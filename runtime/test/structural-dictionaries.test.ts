// oxlint-disable typescript/no-unsafe-type-assertion -- class selection fixes the structural dictionary's method ABI
import assert from "node:assert/strict";
import { test } from "node:test";
import { EQ, structural, type FieldDict } from "../src/core/data.ts";

for (const name of ["Ord", "Hash"]) {
  test(`structural ${name} leaves superclass factories cold until read`, () => {
    let reads = 0;
    const failure = new Error("superclass factory");
    const field: FieldDict = {
      eq: () => () => false,
      compare: () => () => EQ,
      hash: () => 17,
      show: () => "field",
      parse: () => undefined,
      get $Eq(): FieldDict {
        reads++;
        throw failure;
      },
    };
    const dictionary = structural(name, [field]) as Partial<FieldDict>;
    assert.equal(reads, 0);
    if (name === "Ord") {
      const compare = dictionary.compare;
      assert.ok(compare);
      assert.equal(compare([1])([2]), EQ);
    } else {
      const hash = dictionary.hash;
      assert.ok(hash);
      assert.equal(hash([1]), 358);
    }
    assert.equal(reads, 0, "using the own method does not demand Eq");
    assert.throws(() => dictionary.$Eq, (error) => error === failure);
    assert.equal(reads, 1);
    assert.throws(() => dictionary.$Eq, (error) => error === failure);
    assert.equal(reads, 2, "a failed getter is not replaced by a cached value");
  });

  test(`structural ${name} superclass reads retain custom field equality`, () => {
    let reads = 0;
    const field: FieldDict = {
      eq: () => () => false,
      compare: () => () => EQ,
      hash: () => 17,
      show: () => "field",
      parse: () => undefined,
      get $Eq(): FieldDict {
        reads++;
        return field;
      },
    };
    const dictionary = structural(name, [field]) as Partial<FieldDict>;
    assert.equal(reads, 0);
    const first = dictionary.$Eq;
    assert.ok(first);
    assert.equal(reads, 1);
    assert.equal(first.eq([1])([1]), false);
    const second = dictionary.$Eq;
    assert.ok(second);
    assert.equal(reads, 2);
    assert.equal(second.eq([1])([1]), false);
  });
}
