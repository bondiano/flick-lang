---
title: "Индексы строк в GADT"
source_hash: 71d5acf5b0e5d870
---

Результат конструктора может фиксировать или связывать индексы киндов
`ServiceRow`, `ErrorRow` и `CaptureRow`. Тело ветви `match` проверяется
под этими равенствами. Функции по-прежнему нужна полная сигнатура.

```flick
service Db =
  get : Int ->{} Int

type Cmd (r : ServiceRow) a =
  | Value a : Cmd none a
  | Read Int : Cmd {Db} Int

run : Cmd r a ->{r} a
let run command =
  match command with
  | Value value -> value
  | Read key -> Db.get key
```

Равенство в ветви `Read` разрешает `Db`. Оно сохраняет эффектную стадию:
даже вызов с опустевшей строкой требований остаётся эффектным вызовом.
Уточнение не делает ту же реализацию чистой функцией.

```flick fail E001
# service Db =
#   get : Int ->{} Int
# type Cmd (r : ServiceRow) =
#   | Read Int : Cmd {Db}
pure : Cmd r -> Int
let pure command =
  match command with
  | Read key -> Db.get key
```

У равенства может быть несколько остаточных строк. Здесь `s` может быть
пустой или содержать `Db`; ветвь доказывает, что любая допустимая `s`
включается в `{Db}`. Она не выбирает пустой остаток.

```flick
# service Db =
#   get : Int ->{} Int
type Packed (r : ServiceRow) =
  | Packed (Eff s none Int) : Packed {Db | s}

unpack : Packed {Db} ->{Db} Int
let unpack packed =
  match packed with
  | Packed work -> await work
```

Проверка полноты сохраняет эти равенства при проверке остальных частей
кортежа. Здесь первая часть доказывает, что `r` содержит `Db`, поэтому во
второй части не может встретиться `Empty`:

```flick
# service Db =
#   get : Int ->{} Int
type Includes (r : ServiceRow) (s : ServiceRow) =
  | Includes : Includes r {Db | r}

type RowCmd (r : ServiceRow) =
  | Empty : RowCmd none
  | WithDb Int : RowCmd {Db}

read_value : (Includes r r, RowCmd r) -> Int
let read_value pair =
  match pair with
  | (Includes, WithDb value) -> value
```

Скрытые остаточные строки разных вхождений конструктора остаются
независимыми. Части значения связывают только их общие внешние индексы.

То же рассуждение действует для строк ошибок. Локальные обработчики
снимают покрываемые ими требования до проверки внешней сигнатуры.
Равенства остаются внутри ветви, в том числе при проверках, отложенных
до конца объявления. Аргументы сервисов и ошибок остаются инвариантными:
наличие `Store String` в строке не разрешает `Store Int`.

Индексы захватов могут связывать допустимые захваты значений:

```flick
type Share (c : CaptureRow) (d : CaptureRow) =
  | Share : Share c c

share : Share c d -> (Unit -> Int)^c -> (Unit -> Int)^d
let share proof callback =
  match proof with
  | Share -> callback
```

Это не отождествляет разные регионы. Параметры кинда `Region` и параметры
строк записей по-прежнему передаются переменными; паттерны не выводят
новую идентичность региона. Уточнение также сохраняет действующие правила
для обработчиков, foreign-вызовов и изменяемых ссылок.

Это общие правила для JS и Go. Остальные ограничения уточняющих ветвей
описаны в [равенстве типов](../type-equality/) и [GADT](../gadts/).
