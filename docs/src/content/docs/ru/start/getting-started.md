---
title: Начало работы
description: Установите инструменты, создайте приложение и запустите его.
source_hash: c3f53e07fdc6a9a5
---

## Установка

Для локального приложения нужны компилятор Flick, Node.js 24.18 или новее
и pnpm. В macOS и Linux последний релиз ставится в `~/.local/bin` одной
командой:

```sh
curl -fsSL https://bondiano.github.io/flick-lang/install.sh | sh
flick --version
```

или через Homebrew: `brew install bondiano/tap/flick`. Можно также скачать
архив `flick` для своей платформы со страницы
[релизов](https://github.com/bondiano/flick-lang/releases) или
[собрать его из исходников](#из-исходников).

В Windows распакуйте `.zip` и добавьте каталог с `flick.exe` в `PATH`.

`flick` также требует `git` в `PATH` и — при первой установке или проверке
проекта — доступа к github.com: пакет `std`, который проект видит без
объявления, и любую зависимость из `git` компилятор забирает этой командой
по тегу `std-v<версия>` своего релиза. Checkout репозитория или
`FLICK_STD_PATH` с локальным `packages/std` убирают эту загрузку, а
зависимостям по пути сеть не нужна.

Проект закрепляет pnpm в `package.json`, и `flick install` ничего не меняет,
если `pnpm --version` — другая версия. Включите закреплённую:
`corepack prepare pnpm@12.16.1 --activate` (эту версию записывает
`flick new`; если в `package.json` другая, возьмите её) или установите её
через `npm install --global pnpm@12.16.1`. `flick doctor` показывает
найденные версии рядом с теми, что нужны проекту.

### Непрерывная интеграция

Репозиторий — ещё и GitHub Action: он скачивает архив релиза, сверяет его
`.sha256` и добавляет `flick` в `PATH` до конца шага.

```yaml
- uses: actions/checkout@v7
- uses: bondiano/flick-lang@main # закрепите тег релиза, в котором есть action
  with:
    version: latest # или v0.1.0
- run: flick --version
```

### Из исходников

Версию Rust выбирает `rust-toolchain.toml` в checkout:

```sh
git clone https://github.com/bondiano/flick-lang
cd flick-lang
cargo build --release -p flick
./target/release/flick --version
```

Такой компилятор берёт `std` своего checkout, поэтому оставьте checkout там,
где собирали, или задайте в `FLICK_STD_PATH` его каталог `packages/std`.

## Первая программа

Самая маленькая программа печатает строку:

```flick
module Main exposing (main)

main : Unit ->{} Unit
let main () =
  Platform.stdout_line "Hello from Flick"
```

`->{}` обозначает эффектную функцию `main`, которой не требуются сервисы.
Вызовы функций хоста вроде `Platform.stdout_line` пока не обозначаются отдельными
требованиями в сигнатурах;
подробности в разделе [Зачем Flick](../why/#ограничения).

## Проект

Flick создаёт ES-модули JavaScript. Они используют среду выполнения Flick
и Effect 4.1.2; зависимости устанавливаются через pnpm.
Создайте проект и запустите его:

```sh
flick new my-app
cd my-app
flick install
flick run
```

`flick new` создаёт `flick.toml`, `package.json` с закреплёнными версиями
среды выполнения и Effect, `src/App/Main.fl`, импортируемый им модуль
`src/App/Greeting.fl` и его тест `test/App/GreetingTest.fl`:

```flick
module App.Main exposing (main)

import App.Greeting exposing (greet)

main : Unit ->{; AppError} Unit
let main () =
  let name = Platform.args () |> List.head |> Option.with_default "world"
  Platform.stdout_line (greet name)
```

`flick run -- Ada` выводит `Hello, Ada!`.

Для проверки типов без запуска выполните `flick check`. Команда
`flick build` сохраняет JavaScript в `dist/`, а `flick test` запускает
тесты из каталога, указанного в настройках проекта. Во время разработки
используйте `flick dev`: она пересобирает и перезапускает приложение
после изменений. Все команды описаны в
[справочнике командной строки](../../reference/cli/).

## На веб-странице

В Astro и Vite модули `.fl` можно импортировать через `@flick-lang/astro`:
плагин собирает каждый импортированный `.fl` командой `flick build`. Пакет
опубликован в npm и версионируется вместе с компилятором, а исходники лежат
в репозитории Flick
([исходники](https://github.com/bondiano/flick-lang/tree/main/tools/astro-flick)).

```js
// astro.config.mjs
import flick from "@flick-lang/astro";

export default defineConfig({ integrations: [flick()] });
```

```astro
<div id="app"></div>
<script>
  import { runMain } from "@flick-lang/effect-ts";
  import { main } from "../app/Main.fl";

  runMain(main);
</script>
```

[Песочница](../../play/) этого сайта устроена так же: модуль на Flick с видом
на `ui`, а введённый код проверяет и компилирует WebAssembly-версия
компилятора. На её вкладке JavaScript видно, что пишет компилятор, для
отладочного или релизного профиля.
