---
title: Начало работы
description: Установите инструменты, создайте приложение и запустите его.
source_hash: e8b85c6bfbf9c762
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

`->{}` говорит, что `main` работает в среде выполнения и не просит сервисов.
Вызовы хоста вроде `Platform.stdout_line` в сигнатурах пока не называются;
подробности в разделе [Зачем Flick](../why/#где-обещание-заканчивается).

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
лежит в репозитории Flick
([исходники](https://github.com/bondiano/flick-lang/tree/main/tools/astro-flick))
и в npm пока не опубликован.

```js
// astro.config.mjs
import flick from "@flick-lang/astro";

export default defineConfig({ integrations: [flick()] });
```

```astro
<div id="app"></div>
<script>
  import { runMain } from "@flick-lang/runtime";
  import { main } from "../app/Main.fl";

  runMain(main);
</script>
```

[Play](../../play/) на этом сайте устроен так же: интерфейс написан
на Flick с помощью `ui`, а введённый код проверяет и компилирует
WebAssembly-версия компилятора.

В [Transpile](../../transpile/) рядом с исходным кодом можно посмотреть
JavaScript для отладочного и релизного профилей сборки.
