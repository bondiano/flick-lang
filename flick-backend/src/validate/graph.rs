use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::{Policy, id, path, string};
use crate::{ArtifactKind, BuildRequest, BuildResult, InputRef, LaunchKind, Toolchain};

fn tool(tool: &Toolchain, request: &BuildRequest, policy: &Policy) -> Result<(), String> {
    if !policy.toolchains.contains(tool) || !request.toolchains.contains(tool) {
        return Err(format!(
            "backend selected an unapproved toolchain `{}`",
            tool.id
        ));
    }
    if tool.id.is_empty()
        || semver::Version::parse(&tool.version).is_err()
        || tool.sha256.len() != 64
        || !tool.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("backend toolchain is not pinned to a version and SHA-256".into());
    }
    Ok(())
}

pub(super) fn validate(
    request: &BuildRequest,
    result: &BuildResult,
    policy: &Policy,
) -> Result<Vec<usize>, String> {
    let limits = policy.limits;
    if result.steps.len() > limits.steps {
        return Err("backend step count exceeds its limit".into());
    }
    let files = result
        .steps
        .iter()
        .try_fold(result.artifacts.len(), |n, s| {
            n.checked_add(s.outputs.len())
        })
        .ok_or("backend file count overflow")?;
    if files > limits.files {
        return Err("backend file count exceeds its limit".into());
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut artifacts = BTreeMap::new();
    let mut steps = BTreeMap::new();
    let mut sources = BTreeSet::new();
    for input in &request.inputs {
        id(&input.id, &mut ids, limits)?;
        sources.insert(input.id.as_str());
    }
    for reserved in policy.reserved_paths {
        path::insert(reserved, &mut paths, limits)?;
    }
    for artifact in &result.artifacts {
        id(&artifact.id, &mut ids, limits)?;
        path::insert(&artifact.path, &mut paths, limits)?;
        if artifact.bytes.len() > limits.artifact_bytes {
            return Err(format!(
                "backend artifact `{}` exceeds its limit",
                artifact.id
            ));
        }
        artifacts.insert(artifact.id.as_str(), (None, artifact.kind));
    }
    for (i, step) in result.steps.iter().enumerate() {
        id(&step.id, &mut ids, limits)?;
        steps.insert(step.id.as_str(), i);
        tool(&step.toolchain, request, policy)?;
        for output in &step.outputs {
            id(&output.id, &mut ids, limits)?;
            path::insert(&output.path, &mut paths, limits)?;
            artifacts.insert(output.id.as_str(), (Some(i), output.kind));
        }
        let mut env_names = BTreeSet::new();
        for env in &step.environment {
            if !policy.environment.contains(&env.key) || !env_names.insert(&env.key) {
                return Err(format!(
                    "unapproved or duplicate toolchain environment `{}`",
                    env.key
                ));
            }
            string(&env.value, limits)?;
        }
        for argument in &step.arguments {
            string(argument, limits)?;
        }
    }
    let order = schedule(result, &artifacts, &steps, &sources)?;
    let mut launches = BTreeSet::new();
    for launch in &result.launches {
        string(&launch.name, limits)?;
        if launch.name.is_empty()
            || !launches.insert((&launch.name, matches!(launch.kind, LaunchKind::Test)))
        {
            return Err("empty or duplicate backend launch name".into());
        }
        let &(_, kind) = artifacts
            .get(launch.artifact.as_str())
            .ok_or("backend launch names an unknown artifact")?;
        if let Some(runtime) = &launch.runtime {
            tool(runtime, request, policy)?;
        } else if kind != ArtifactKind::Executable {
            return Err("backend launch without a runtime needs an executable artifact".into());
        }
        for argument in &launch.arguments {
            string(argument, limits)?;
        }
    }
    let mut dependencies = BTreeSet::new();
    for dependency in &result.runtime_dependencies {
        if !request.runtime_dependencies.contains(dependency)
            || !dependencies.insert(&dependency.package)
        {
            return Err("backend runtime dependency is unpinned, undeclared or duplicated".into());
        }
    }
    for step in &result.steps {
        (policy.authorize_step)(step)?;
    }
    Ok(order)
}

type ArtifactIndex<'a> = BTreeMap<&'a str, (Option<usize>, ArtifactKind)>;

fn schedule(
    result: &BuildResult,
    artifacts: &ArtifactIndex,
    steps: &BTreeMap<&str, usize>,
    sources: &BTreeSet<&str>,
) -> Result<Vec<usize>, String> {
    let mut degrees = vec![0; result.steps.len()];
    let mut children = vec![Vec::new(); result.steps.len()];
    for (i, step) in result.steps.iter().enumerate() {
        let mut deps = BTreeSet::new();
        for dep in &step.dependencies {
            let &j = steps
                .get(dep.as_str())
                .ok_or("backend step depends on an unknown step")?;
            if !deps.insert(j) {
                return Err("duplicate backend step dependency".into());
            }
            degrees[i] += 1;
            children[j].push(i);
        }
        let mut read = BTreeSet::new();
        for input in &step.inputs {
            let (namespace, input) = match input {
                InputRef::Source(s) => (0, s),
                InputRef::Artifact(s) => (1, s),
            };
            if !read.insert((namespace, input)) {
                return Err("duplicate backend step input".into());
            }
            if namespace == 0 {
                if !sources.contains(input.as_str()) {
                    return Err("backend step reads an undeclared side input".into());
                }
            } else {
                let &(producer, _) = artifacts
                    .get(input.as_str())
                    .ok_or("backend step reads an unknown artifact")?;
                if let Some(producer) = producer
                    && !deps.contains(&producer)
                {
                    return Err(
                        "backend step reads an output without declaring its dependency".into(),
                    );
                }
            }
        }
    }
    let mut ready: VecDeque<_> = degrees
        .iter()
        .enumerate()
        .filter_map(|(i, &n)| (n == 0).then_some(i))
        .collect();
    let mut order = Vec::new();
    while let Some(i) = ready.pop_front() {
        order.push(i);
        for &child in &children[i] {
            degrees[child] -= 1;
            if degrees[child] == 0 {
                ready.push_back(child);
            }
        }
    }
    if order.len() != result.steps.len() {
        return Err("backend build graph has a cycle".into());
    }
    Ok(order)
}
