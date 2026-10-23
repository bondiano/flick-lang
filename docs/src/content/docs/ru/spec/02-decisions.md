---
title: "2. Решения первой версии"
---

| Область | Принято | Цена выбора |
| - | - | - |
| Sequencing | Прямые вызовы с let; task/await; без generic do | Для пользовательских монад обычные combinators |
| Тип вычисления | Eff r e a, результат последним | Source order отличается от native TS order |
| Классы | MPTC, superclasses, FD; canonical instances | Strict coverage/termination; часть instance chains не переносится |
| Coherence | No overlap/orphans/local implicits; package ownership | Для альтернатив нужны wrappers или explicit dictionaries |
| Generalization | Только доказанно pure построения; runtime results мономорфны | Иногда требуется фабрика; variance-generalization отложена |
| Higher-order API | map и последовательный map_effect; явный parallel limit | Два семейства операций вместо скрытого runtime bridge |
| Collections | List = dense immutable JS array; отдельный Effect Chunk | Prepend/tail/update обычно O(n) |
| Records | Exact structural + row polymorphism | Нет implicit width subtyping и silent overwrite |
| Type-level | Fixed row primitives, labels, finite kinds, MPTC/FD | Общие type families и associated types отложены |
| Lifetime | Fresh regions, captures, без early close | Закрытие ждёт children и foreign quiescence |
| Errors | Native recovery; opaque Failure/Exit/RawCause; region-free typed payloads | Raw failures нельзя считать остаточной e |
| Concurrency | Structured combinators, scoped supervisor, native race/timeout | Нет nursery, detached scoped work и hard deadline |
| FFI | Fixed ABI, четыре callback families, checked `.d.ts` subset | Purity/retention/cancellation требуют contracts |
| Schema | Full deriving, explicit codecs, strict exporters | Не всякая schema выражается во внешнем формате |
| SQL | Pure DSL, PostgreSQL, Query/Template | Зафиксирован ограниченный SQL subset |
| UI | ui-блок, React adapter, explicit event policy | Без JSX, собственного renderer, SSR и полной hooks-модели |
| CLI-приложения | cli-блок поверх Effect CLI, точный record опций, `Cli.Value` для разбора | Подкоманда не видит опции родителя; без собственного парсера аргументов |
| Compiler | Rust core Flick, typed IR, source LSP, versioned JSON iface | Собственный checker, Mapper — слой интеграции |
| Optimization | Debug baseline и typed release passes из подходов ReScript | Нужны доказательства сохранения observable contracts |
| CLI | `flick`, `.fl`, `flick.toml`, один pnpm lockfile; check/build/run/dev/test/fmt и инструменты FFI/Schema | Команды разделяют один checker; версии runtime и интерфейсов проверяются явно |

Все перечисленные решения относятся к первой версии. Pure `a -> b`, runtime `a ->{r; e} b` и runtime без requirements/errors `a ->{} b` различаются в checker и IR. Статические guarantees действуют для проверенного source и правдивых trusted bindings; native Effect и Content Mapper не доказывают их автоматически.
