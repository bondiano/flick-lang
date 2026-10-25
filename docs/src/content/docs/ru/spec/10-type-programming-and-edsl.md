---
title: "10. Типовое программирование и eDSL"
---

**Принято для первой версии:** библиотечные SQL/UI/CLI DSL с выбранным ML-синтаксисом `sql params { ... }`, `ui { ... }` и `cli "name" { ... }`; PostgreSQL backend, React adapter и Effect CLI. Типовое ядро общее: rows, labels, finite indices, HKT и MPTC/FD. Примеры ниже — синтаксис Flick, не Selda или Droplet.

## Что показывают Selda и Droplet

[Selda](https://github.com/Kamirus/purescript-selda) строит SQL через чистую монаду состояния и типизированные выражения колонок; [Droplet](https://github.com/easafe/purescript-droplet) ближе к SQL-комбинаторам, где тип AST содержит clauses, поля и проекцию. Обе библиотеки связывают структуру запроса с типом результата через rows, multiparameter classes и functional dependencies.

| Задача | Наблюдение по исходникам | Решение для Flick |
| - | - | - |
| Создание запроса | У Selda `Query s a` оборачивает чистый `State GenState a` | Построение AST чистое; обращение к БД — отдельный runtime-вызов |
| Поля таблицы и результата | Row преобразуется в record SQL-выражений, затем в record значений | Универсальные операции над rows; пользователь работает с обычными полями |
| LEFT JOIN | Обе библиотеки делают правую сторону nullable, не удваивая Maybe | Nullability как отдельный индекс SQL-выражения; на выходе `Option a` |
| Вывод проекции | У Droplet `ToQuery q projection` имеет зависимость `q -> projection` | Тип результата однозначно вычисляется из завершённого запроса |
| Скрытие большого типа | Внутренний `buildQuery` Droplet стирает AST в SQL и параметры, сохраняя projection | Публичный opaque Query с короткой сигнатурой, без необходимости писать тип всего AST |
| Разбор частных случаев | В исходниках есть ordered `else instance` chains | Прямой порт конфликтует с no-overlap; нужны непересекающиеся индексы и детерминированные row/type операции |

Проверены исходники Selda на commit `a81f47f9054e9ea893d5ef79586be70068462249`: [Query.Type](https://github.com/Kamirus/purescript-selda/blob/a81f47f9054e9ea893d5ef79586be70068462249/src/Selda/Query/Type.purs), [FromTable и nullable JOIN](https://github.com/Kamirus/purescript-selda/blob/a81f47f9054e9ea893d5ef79586be70068462249/src/Selda/Query.purs#L197-L302). Для Droplet — commit `990dd8ce078436bdb6c9ffe280dfdc97c516802b`: [ToQuery](https://github.com/easafe/purescript-droplet/blob/990dd8ce078436bdb6c9ffe280dfdc97c516802b/src/Droplet/Language/Internal/Translate.purs#L99-L145), [nullable instance chains](https://github.com/easafe/purescript-droplet/blob/990dd8ce078436bdb6c9ffe280dfdc97c516802b/src/Droplet/Language/Internal/Syntax.purs#L1507-L1516), [внутренний buildQuery](https://github.com/easafe/purescript-droplet/blob/990dd8ce078436bdb6c9ffe280dfdc97c516802b/src/Droplet/Language/Internal/Translate.purs#L1161-L1172).

С SQL в целевом наборе выбраны ограниченные MPTC/FD и compiler-owned row primitives. Непересекающиеся индексы и typed AST заменяют случаи, где исходные PureScript-библиотеки используют ordered instance chains. Побуквенный перенос таких instances не обещается; исходные правила coherence сохраняются.

## Минимальные возможности на уровне типов

| Механизм первой версии | Что выражает | Граница |
| - | - | - |
| Type, FieldRow = Row Type, Symbol, Constraint, Region, CaptureRow | Данные, поля, constraints и lifetime | Разные row kinds не смешиваются |
| Пользовательские finite kinds | Закрытые nullability/phase indices | Только конечные нульарные constructors; без general promotion/dependent values |
| Точные structural records и row polymorphism | Известные поля и открытый хвост | Нет implicit width subtyping |
| HasField, Lacks, Project, Remove, UnionDisjoint | Доступ, projection, отсутствие и коллизии полей | Compiler-owned deterministic primitives |
| Map по row и canonical labels | Преобразование каждого field-type конструктором Type → Type | Не общий язык type families |
| MPTC, superclasses, FD | Связи AST, проекции и parameters | Strict coverage, non-overlap, termination |
| Opaque indexed AST | Scope, phase, nullability и checked Query | Нельзя обойти через публичный constructor |
| Higher-rank callbacks и captures | Fresh DSL scopes и callbacks | Явные forall; provenance сохраняется |
| Диагностические constraints | Ошибка с label и source range | Не произвольный compile-time JS |

HKT сам по себе не вычисляет, какие поля появятся после JOIN. Higher-rank сам по себе не преобразует record. Нужны именно операции над структурой типов.

Пользовательский конечный kind объявляется, например, `kind Nullability = NonNull | Nullable`; его constructors — только type-level indices. Такие декларации не создают runtime values или общий механизм promotion. Пользовательские open/closed type functions, associated types, instance chains, impredicativity и зависимые типы не входят в первую версию. Если relation не проходит strict coverage, библиотека переносит вычисляемую проекцию в параметр typed AST или использует fixed row primitive, а не включает UndecidableInstances.

### Три способа реализовать type-level часть

| Подход | Выгода | Цена |
| - | - | - |
| Особый SQL checker в компиляторе | Быстрый путь к хорошим SQL-ошибкам | Компилятор знает домен; новым DSL нужны новые специальные правила |
| Библиотечные relations на MPTC/FD и row primitives | Расширяемость обычными библиотеками; тот же механизм нужен Schema и UI | Нужны termination/coherence и понятные diagnostics |
| Закрытые функции над типами | Удобная запись преобразований вроде projection/defaults/nullability | Отдельный normalizer и правила редукции равенств; открытые входы могут оставаться невычисленными |

Выбран второй путь: общие row primitives и библиотечные relations. SQL elaborator переводит surface syntax в typed constructors и прикрепляет исходные диапазоны; PostgreSQL-семантика не становится набором специальных правил универсального checker. Закрытые type functions исключены из первой версии.

Нормализация кешируется и имеет явный бюджет: начальный default — 100 000 solver steps на проверяемое определение и глубина 256 class constraints. Build configuration может повысить предел; превышение даёт diagnostic с цепочкой constraints. Бюджет не заменяет правила завершения и не позволяет принять непроверенный код. Type-level вычисления не запускают произвольный JS и не обращаются к сети или БД.

## SQL: короткая сигнатура, полный вывод результата

Public API использует разные opaque-типы:

- `Sql.Template dialect params result` — план формы запроса с именованными ещё не связанными параметрами.

- `Sql.Query dialect result` — запрос со связанными значениями параметров.

- Внутренние fragments/expressions — промежуточные типы с индексами scope и фазы; непосредственно выполнить их нельзя.

`Template` не обещает server-side PREPARE: это отдельная оптимизация драйвера. Граница завершения запроса проверяет все ссылки и clauses, затем скрывает большой тип AST. Это сохраняет наше правило полной сигнатуры над каждой именованной функцией и позволяет обойтись без `_` в публичном типе запроса.

Допустим, описаны таблицы Users и Accounts. `Users.id` и `Accounts.user_id` имеют тип UserId, `Users.name` — String, исходный `Accounts.balance` — non-null BigDecimal.

```flick
type alias BalanceRow =
  {
    name : String
    balance : Option BigDecimal
  }

balances :
  Sql.Template Pg { user_id : UserId } BalanceRow
let balances =
  sql params {
    from user in Users
    left_join account in Accounts
      on account.user_id == user.id
    where user.id == params.user_id
    select {
      name = user.name
      balance = account.balance
    }
  }

load_balances :
  UserId ->{Database; EncodeError, DbError, DecodeError} List BalanceRow
let load_balances user_id =
  let query = Sql.bind balances { user_id = user_id }
  Database.query query
```

`params` — binder выбранной SQL-грамматики; его поля выводятся из ожидаемого Template. `Sql.bind` чисто связывает инертные значения, не выполняя I/O. В первой версии codecs параметров/ответов сертифицированы как pure; их expected failures не теряются. `Database.query` имеет контракт `Sql.Query Pg a ->{Database; EncodeError, DbError, DecodeError} List a`: кодирует параметры, исполняет запрос и проверяет ответ. Effectful enrichment выполняется после запроса обычным runtime-кодом; serviceful SQL codecs отложены.

Тип balance становится `Option BigDecimal` именно из-за LEFT JOIN. Если заменить JOIN на INNER JOIN, nullable-причина этого соединения исчезнет, но исходно nullable-колонка всё равно останется nullable. Повторный LEFT JOIN не должен давать вложенные `Option (Option a)` лишь из-за двух одинаковых причин отсутствия.

Вызов `load_balances` обычный прямой эффектный вызов. Сохранение его на потом выглядит так:

```flick
deferred_balances :
  UserId -> Eff {Database} {EncodeError, DbError, DecodeError} (List BalanceRow)
let deferred_balances user_id =
  task { load_balances user_id }
```

`sql params { ... }` строит чистый AST; без сахара доступны те же библиотечные combinators. Обычный let не связывает произвольную монаду. Generic do в первую версию не входит.

Блок `sql params { … }` состоит из clauses — по одной на строку или через `;`; продолжение clause пишется с большим отступом. Необязательное имя после `sql` связывает параметры запроса. `from x in T`, `join x in T` и `left_join x in T` вводят alias; `on e` уточняет предыдущий join и может стоять на следующей строке. Остальные clauses — слово и выражения через запятую: `where`, `group_by`, `having`, `order_by`, `limit`, `offset`, `select`, `union`, `union_all`, `insert_into`, `values`, `update`, `set`, `delete_from`, `returning`. Внутри блока эти слова завершают применение функции, поэтому clause может начинаться без разделителя. Parser фиксирует только эту форму; типы, scopes, фазы и диалект проверяет SQL elaboration.

### Что проверяется в SQL

| Проверка | Пример отклоняемой программы |
| - | - |
| Поля и типы колонок | `user.nmae`; сравнение UserId с Date |
| Scope и aliases | Ссылка на alias соседнего завершённого запроса; неоднозначное неквалифицированное имя |
| Проекция | Два результата с одним именем без явной политики переименования |
| Nullable JOIN | Попытка получить plain BigDecimal из nullable balance |
| Фазы и агрегации | Неагрегированное поле в SELECT агрегатного запроса без допустимой группировки |
| INSERT | Отсутствующее обязательное поле; значение неправильного типа; запись в generated-always поле |
| Диалект | Операция, которой нет в выбранном backend |
| Параметры | Неизвестное имя параметра; неподходящий codec |

Внутренний opaque `Sql.Expr scope phase nullability sql_type` сохраняет indices; его constructors принадлежат DSL. LEFT JOIN меняет nullability, group_by создаёт свидетельство группировки, aggregate expression нельзя подставить в WHERE. Alias scopes fresh; завершённый Query не выпускает локальные expressions. Первая версия поддерживает только некоррелированные подзапросы; outer correlation и LATERAL вынесены за её пределы.

SQL использует свою трёхзначную логику. `==` внутри SQL-блока преобразуется в SQL-сравнение, а не в обычный `Eq` языка. WHERE сохраняет строки с SQL TRUE; SQL UNKNOWN не превращается автоматически в source Bool. Нужны отдельные `is_null`, `is_not_null` и null-safe equality. Агрегаты также имеют собственные правила: MAX по пустому набору может дать NULL, COUNT — другой контракт.

### Данные и SQL-текст

Параметры и identifiers — разные каналы. Значения пользователя передаются placeholders и массивом параметров драйвера; они не конкатенируются в SQL. Имена таблиц, aliases и колонок поступают из metadata или проверенного Identifier и корректно кодируются backend. Runtime String не превращается неявно в SQL-фрагмент.

Raw SQL остаётся отдельным явно unsafe API с заданным result codec. Он не получает статические гарантии typed AST. Нельзя называть параметризацией подстановку пользовательского имени таблицы в позицию значения.

### Граница Schema и SQL

Первый dialect — **PostgreSQL**. Table metadata явно задаёт SQL types, nullable, keys, generated/identity/default и связь с чистыми domain/wire codecs. Metadata пишется в source либо генерируется отдельным инструментом из миграций/схемы; обычная компиляция не подключается к БД. Domain Schema не выводит SQL constraints автоматически и не превращает любой refinement в CHECK.

| SQL subset | Первая версия |
| - | - |
| SELECT/FROM/WHERE и явная projection | Да |
| INNER JOIN и LEFT JOIN | Да |
| GROUP BY/HAVING, основные агрегаты | Да, с проверкой phases |
| ORDER BY, LIMIT/OFFSET | Да |
| Некоррелированные subqueries, EXISTS/IN | Да |
| UNION/UNION ALL | При совместимых projections |
| INSERT/UPDATE/DELETE и RETURNING | Да; required/default/generated columns проверяются |
| Correlated/LATERAL, recursive CTE, window functions | После первой версии |
| RIGHT/FULL JOIN, произвольные extensions, DDL/migration DSL | После первой версии |

Вне safe subset доступен отдельно названный unsafe/raw API с явным result decoder.

SQL bigint отображается в BigInt, decimal — BigDecimal; Number допустим только через явно проверяемый диапазон/precision. SQL NULL — индекс expression и Option на выходе; source Option как доменное значение не подменяет этот индекс. Schema drift и mismatch ответа дают DecodeError, кодирование параметра — EncodeError, driver/server/constraint failures — DbError с инертной диагностикой. Defects и interruption сохраняют общий runtime-контракт.

Отмена не обещает откат уже выполненного SQL. Driver не возвращает connection в pool до согласованного drain/close/quiescence. Типы не доказывают актуальность БД, права, стоимость плана, transaction isolation или число строк без отдельного cardinality contract.

## UI eDSL: типы props, children и действий

Базовая модель — `Ui r e`: чистое описание интерфейса с отложенными обработчиками, которым нужны сервисы r и которые могут завершиться ошибками e. Построение UI не выполняет эти обработчики.

```flick
type alias SaveButtonProps c =
  {
    label : String
    disabled : Bool
    on_save : (Unit ->{Users; SaveError} Unit)^c
  }

save_button :
  SaveButtonProps c -> (Ui {Users} {SaveError})^c
let save_button props =
  Ui.button
    {
      disabled = props.disabled
      on_click = Ui.Event.exhaust (fun _ ->
        task { props.on_save () }
      )
    }
    [Ui.text props.label]
```

`on_click` имеет opaque тип `(Ui.Handler ClickSnapshot r e)^c`. Его builder принимает callback, создающий холодный Eff, и явно выбранную concurrency policy. В примере Exhaust игнорирует новые клики до завершения текущей задачи. Pure `save_button` не выполняет `props.on_save` или Ref.get при render; state поступает через модель/props. Общие effectful hooks внутри pure render не разрешены. Captures props/handler сохраняются в Ui.

Это согласуется с требованием повторяемого render у [React](https://react.dev/reference/rules/components-and-hooks-must-be-pure), но проверка чистоты и lifetime — контракт Flick.

### Синтаксис UI во Flick

`ui { ... }` — выбранный сахар над теми же проверяемыми вызовами:

```flick
save_button :
  SaveButtonProps c -> (Ui {Users} {SaveError})^c
let save_button props =
  ui {
    button
      {
        disabled = props.disabled
        on_click = Ui.Event.exhaust (fun _ ->
          task { props.on_save () }
        )
      }
      [text props.label]
  }
```

JSX `<...>` не добавляется в первую версию; `ui { ... }` и обычные Ui functions строят одно представление: блок только открывает имена модуля `Ui` (локальная привязка сильнее, затем имя `Ui`, затем имена модуля) и даёт тот же typed IR. Это оставляет ML-синтаксис и единые правила props/callbacks. [ReScript components](https://rescript-lang.org/docs/react/components-and-props/) служат примером типизированного façade к React; surface syntax Flick выбран независимо.

Первый renderer — **React adapter**. Собственный reconciler/DOM renderer, SSR, hydration, React Server Components и полная hooks-модель вынесены за первую версию. Adapter сохраняет component boundaries, key/ref и lifecycle React; optimizer не заменяет компонент произвольным прямым вызовом функции. Runtime состояние bridge находится вне pure component body.

### Props и children

Props — точный structural record. Отсутствующее обязательное поле, опечатка или неверный тип — ошибка. Spread использует disjoint row merge; overwrite только через явный update. Optional/defaults следуют общему языку, а JS undefined появляется лишь в adapter. Children — обычный аргумент выбранного типа. Renderer key/ref — отдельная metadata, не скрытые поля пользовательского props record.

| Children-контракт | Тип данных |
| - | - |
| Любое число узлов | `List (Ui r e)` |
| Ровно один узел | `Ui r e` |
| Необязательный узел | `Option (Ui r e)` |
| Именованные места | Record с header/body/footer |
| Непустой набор вкладок | `NonEmpty (Tab r e)` |
| Элемент без содержимого | В API нет аргумента children |

Если компонент должен принимать именно Tab, сохраняем тип Tab до сборки контейнера. После стирания любого child в общий Ui выяснять его происхождение слишком поздно. Уникальность key в динамическом списке обычная сигнатура также не доказывает.

Обычный generic-компонент не требует общих type families:

```flick
type alias SelectProps a r e c =
  {
    items : List a
    selected : Option a
    key : (a -> String)^c
    render_item : (a -> (Ui r e)^c)^c
    on_change : (Ui.Handler a r e)^c
  }
```

`on_change` — обработчик с обязательной policy (`Ui.Event.latest …`), snapshot события — выбранный элемент: голая функция `a -> Eff r e Unit` означала бы неявный fire-and-forget. Все поля связаны одним a. Нельзя совместить items из User и обработчик для Product. Row operations потребуются для универсальных wrappers, defaults и контролируемого spread; higher-rank — для специальных полиморфных render callbacks, а не для каждого generic-компонента. В [TypeScript JSX](https://www.typescriptlang.org/docs/handbook/jsx.html) также разделены проверка intrinsic props и children; DSL Flick может выразить эти контракты обычными функциями без воспроизведения всей системы типов TS.

### Исполнение событий и время жизни

DOM callback, который ожидает void, не исполняет возвращённый Eff. Адаптер обязан получить инертный event snapshot, синхронно принять решение о default action, запустить управляемую задачу и наблюдать её завершение. `preventDefault` нельзя откладывать до произвольного await; подходят декларативная политика либо чистый синхронный decision callback.

`e` в Ui описывает selected expected errors будущих обработчиков. UI root обязательно получает supervisor полного `Failure e`; инфраструктурные defects и shutdown interruption различимы. `Ui.Dom.container id` ищет необязательный `Ui.Dom.Container`. `Ui.Dom.mount container supervisor ui` рендерит `ui` с реестром `Reactivity.Atom.Registry`; представление (`Ui.view`) читает атомы через `Atom.Read` во время render и перерисовывается только при изменении прочитанного атома, а обработчики пишут атомы через `Atom.set`/`Atom.update`. Первое монтирование DOM синхронно. React 19 может объединять последующие рендеры; это не обещание commit после каждой задачи или одного рендера на кадр анимации. `Ui.Dom.mount` живёт до закрытия root и завершения cleanup. Завершившийся mount не обещает вернуть будущие failures через свой обычный результат. Сбой самого supervisor завершает root как defect и передаётся runtime reporter, без рекурсивного reporting.

`Ui.mount_with renderer supervisor ui` имеет тот же контракт реестра, сервисов, ошибок, захватов и завершения, используя непрозрачную фабрику `Ui.Renderer` вместо DOM-контейнера. Фабрика открывает корень только при запуске Effect и предоставляет операции `render(node)` и синхронного `unmount()`. `Ui` импортирует только `@flick-lang/runtime/ui/react`; за монтирование DOM отвечают `Ui.Dom` и `@flick-lang/runtime/ui`, поэтому скомпилированному потребителю `Ui.mount_with` не нужен `react-dom`. HTML-конструкторы остаются DOM-специфичными. Адаптеры React Native, Ink и React Three Fiber и их типизированные host primitives ещё не реализованы.

Ключи соседних узлов сохраняют идентичность поддерева и политик событий при перестановке, а стабильные listeners компонентов получают обработчик завершённого commit. Представления над атомами с обработчиками Effect — Elm-like, а не полная TEA: последовательный цикл сообщений, команды из `init`/`update` и подписки по модели ещё отсутствуют. Приоритеты и свидетельства реализации — в [справочнике UI](../../reference/ui/#приоритеты-недостающих-возможностей).

Policy обязательна у каждого обработчика; неявного fire-and-forget нет.

| Constructor | Поведение |
| - | - |
| `Ui.Event.exhaust` | Игнорирует новые события, пока текущая задача не завершилась |
| `Ui.Event.latest` | Отменяет прежнюю, ждёт остановки/quiescence, затем запускает последний snapshot; pending slot один |
| `Ui.Event.queue capacity` | Ограниченная FIFO; при заполнении новое событие отвергается, supervisor получает infrastructure overflow defect |
| `Ui.Event.concurrent limit` | Не более limit активных задач; при насыщении новое событие отвергается с infrastructure overflow defect, без скрытой неограниченной очереди |

Capacity/limit — проверенные положительные целые; константы проверяются compiler, динамические значения — checked constructor. Infrastructure defects не выдаются за новый элемент typed row `e`. Отмена не откатывает уже совершённое внешнее действие. Default action сохраняется, если декларативная policy или pure synchronous decision не требует preventDefault.

Capture checking проходит через props, callbacks, Eff и Ui. Вернуть кнопку, обработчик которой захватил file из уже закрытого scope, — ошибка. Допустимо открыть файл внутри task либо удерживать UI в области жизни файла. Наличие `Ui none e` не доказывает отсутствие захваченного scoped-ресурса.

React cleanup синхронно закрывает приём событий и снимает registrations. App-owned close task прерывает и joins owned fibers, дожидается foreign quiescence и лишь затем освобождает resources. `Ui.Dom.mount` и `Ui.mount_with` удерживают app owner до конца этой работы. Рендерер должен размонтировать свои компоненты React, а не просто скрыть поверхность. Сам React cleanup callback не объявляется обещанием дождаться async finalizers. [React useEffect](https://react.dev/reference/react/useEffect) задаёт lifecycle cleanup; двухфазное ожидание — дополнительный контракт bridge Flick.

### Безопасность содержимого и interop

`Ui.text : forall r e. String -> Ui r e` (полиморфна по строкам, чтобы стоять среди детей любого элемента) вставляет текст через безопасный текстовый канал. URL и raw HTML имеют отдельные политики и непрозрачные типы, например SafeUrl и SanitizedHtml. Их конструкторы проверяют вход или являются явно доверенными; простое приведение String не создаёт доказательство безопасности. Это разделение каналов, а не обещание автоматически доказать всю HTML-грамматику, accessibility и корректность приложения.

`.d.ts` может дать shape props foreign component, но не доказывает purity, retention и cancellation. Foreign component хранится как opaque renderer component; pure source builder его не исполняет. Adapter задаёт callback/lifetime policy и owner. Импортируемая поддержанная часть React-экосистемы проходит тот же FFI pipeline; непредставимые гарантии не стираются.

## CLI eDSL: флаги, аргументы и подкоманды

Приложение командной строки описывается значением `Cli.Command r e`: чистым описанием флагов, позиционных аргументов, подкоманд и отложенного handler, которому нужны сервисы r и который может завершиться ошибками e. Построение команды ничего не разбирает и не запускает. Runtime-часть — [Effect CLI](https://effect.website/docs/v4/api/effect/cli/Command); help, автодополнение и разбор берутся из неё, а типы опций, полнота и captures — контракт Flick.

```flick
greet : Cli.Command {Platform} none
let greet =
  cli "greet" {
    description "Печатает приветствие"
    /// Печатать заглавными буквами
    flag loud : Bool
    /// Сколько раз повторить
    flag times "n" : Number = 1
    /// Кому адресовано приветствие
    arg name : String
    handle fun { loud; times; name } ->
      let text = "Hello, " ++ name
      let line = if loud then String.to_upper text else text
      Platform.print_line (String.join "\n" (List.repeat times line))
  }
```

`cli "name" { … }` — сахар над функциями `Cli.command`, `Cli.flag`, `Cli.arg`, `Cli.description`, `Cli.subcommands` и `Cli.handle`; без сахара доступны те же combinators, и обе формы строят одно представление. Блок состоит из clauses — по одной на строку или через `;`:

| Clause | Смысл |
| - | - |
| `description e` | Текст help команды, `e : String`; не больше одной |
| `flag name "alias"… : T [= default]` | Именованный параметр `--name`; строки после имени — дополнительные имена |
| `arg name : T [= default]` | Позиционный аргумент; порядок clauses — порядок на командной строке |
| `subcommand e` | Вложенная команда, `e : Cli.Command r e` |
| `handle f` | Обработчик, `f : Options ->{r; e} Unit` |

Слова clauses контекстные: они особые только в начале clause внутри `cli { … }`. Doc-comment `///` непосредственно над `flag`, `arg` или `subcommand` становится его текстом в help; тот же текст показывает `flick doc`.

### Опции как точный record

Elaboration выводит точный record опций: по полю на каждый `flag` и `arg` в порядке объявления, имя поля — имя clause. В примере handler получает `{ loud : Bool; times : Number; name : String }`; опечатка в record pattern или обращение к несуществующему полю — обычная ошибка типов. Имя на командной строке получается из имени поля заменой `_` на `-`: `dry_run` → `--dry-run`. Дополнительное имя из одного символа даёт короткую форму `-n`, более длинное — `--name`; дефисы в строке не пишутся.

Тип значения задаёт и разбор, и кратность:

| Тип в clause | Поведение |
| - | - |
| `Bool` | Флаг-переключатель; без `= default` равен `false` |
| `T` | Обязательный параметр, если нет `= default` |
| `Option T` | Необязательный; `= default` запрещён |
| `List T` | Повторяемый флаг (0 и больше); у `arg` — хвост переменной длины, только последним |
| `NonEmpty T` | Как `List T`, но хотя бы одно значение |

Для `T` нужен instance класса `Cli.Value T`, который разбирает строку в typed ошибку. Библиотека даёт instances для `String`, `Number` (отвергает NaN и пустую строку), `BigInt`, `Path`, `Duration` и `Redacted String`; значение `Redacted` не печатается в help, ошибках и логах. `deriving Cli.Value` строит instance для ADT из нульарных constructors — выбор из имён constructors в kebab-case, — и для refined type, если instance есть у базового типа: разбор вызывает `T.parse`. Default — чистое выражение типа `T`; в help он показывается через `Show T`, если такой instance есть.

### Подкоманды и handler

```flick
tool : Cli.Command {Platform} none
let tool =
  cli "tool" {
    description "Утилиты проекта"
    /// Подробный вывод
    flag verbose "v" : Bool
    subcommand greet
    subcommand version
  }
```

Сервисы и ошибки команды — объединение rows её handler и подкоманд, как у любого составного выражения. Handler подкоманды получает только собственный record опций; флаги родителя разбираются до имени подкоманды и видны только handler родителя. Общая конфигурация для всех подкоманд передаётся сервисом, предоставленным в `main`. Команда без `handle` обязана иметь подкоманды; её вызов без подкоманды печатает help и завершается `CliError`. Команда с подкомандами не объявляет `arg`, иначе позиционный аргумент неотличим от имени подкоманды.

Ошибки компиляции: повтор имени поля, флага или подкоманды, совпадение длинного или короткого имени, в том числе с зарезервированными `--help`, `-h`, `--version` и `--completions`; `= default` у `Option`; `List`/`NonEmpty` у `arg` не последним; тип без `Cli.Value`; default не того типа; `handle` или `description` дважды; команда без `handle` и без подкоманд; `arg` рядом с `subcommand`.

### Запуск

`Cli.run : Cli.Command r e -> List String ->{Platform | r; CliError | e} Unit` разбирает аргументы чистой функцией и только после успешного разбора выполняет handler в текущем fiber. `--help`, `--version` и `--completions bash|zsh|fish` печатают результат в stdout и завершаются без вызова handler. Неверный ввод — неизвестный флаг, отсутствующий обязательный аргумент, значение, которое `Cli.Value` не разобрал, — даёт `type error CliError = CliError String` с текстом для пользователя; handler не запускается.

```flick
main : Unit ->{; AppError} Unit
let main () =
  provide Platform.live {
    try
      Cli.run tool (Platform.args ())
    with
    | CliError message -> App.fail message
  }
```

Команда остаётся обычным значением: entry point по-прежнему один `main : Unit ->{; AppError} Unit`, коды выхода и порядок cleanup — общие правила раздела 12. Отмена по SIGINT прерывает handler, и его ресурсы закрываются по контракту owner из раздела 7. Captures проходят через handler так же, как через callbacks UI: команду, handler которой захватил ресурс закрытого scope, нельзя вернуть из этого scope. Интерактивные запросы (`Cli.Prompt`) — обычные эффектные функции внутри handler, а не clauses блока.

## Что SQL и UI добавляют к проверке дизайна

Принятый дизайн проверяют два обязательных сквозных прототипа; их результат может выявить дефект спецификации, но выбор механизмов уже сделан:

- SQL: typed table → LEFT JOIN → nullable projection → bind parameters → Effect execution; отрицательные примеры с неизвестным полем, неверной группировкой, утёкшим alias и неверным параметром.

- UI: generic props → typed children → pure render → task callback → управляемое завершение; отрицательные примеры с неверным prop, runtime-вызовом в render и захватом закрытого ресурса.

Они проверяют выразительность и стоимость inference. Для больших rows и вложенных запросов измеряются время проверки и качество диагностики; успешные маленькие примеры не доказывают масштабируемость. Оптимизации могут заранее скомпилировать статическую форму SQL и чистое создание UI, сохраняя параметры, capture metadata, границы renderer и наблюдаемое выполнение Effect.
