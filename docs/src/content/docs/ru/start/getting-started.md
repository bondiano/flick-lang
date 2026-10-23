---
title: Начало работы
description: Установите инструменты, создайте приложение и запустите его.
---

Flick компилируется в ES-модули, работающие на Node.js 24.18 или новее
со средой выполнения Flick и Effect 4.0.5. Пакеты устанавливаются через pnpm.

```sh
flick new my-app
cd my-app
flick install
flick run
```

`flick new` создаёт `flick.toml`, `package.json` с закреплёнными версиями
среды выполнения и Effect, а также `src/App/Main.fl`:

```flick
module App.Main exposing (main)

main : Unit ->{; AppError} Unit
let main () =
  Platform.stdout_line "Hello from Flick"
```

`flick check` проверяет типы, ничего не запуская; `flick build` записывает
JavaScript в `dist/`; `flick test` запускает тесты из корневого каталога тестов;
`flick dev` пересобирает и перезапускает приложение после каждого изменения.
Все команды перечислены в [справочнике командной строки](../../reference/cli/).

## На веб-странице

Astro и Vite импортируют модули Flick напрямую через `@flick-lang/astro`:
плагин собирает каждый импортированный `.fl` с помощью `flick build`.

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

[Песочница](../../playground/) этого сайта — именно такая программа: модуль
Flick с представлением `ui`, который компилирует ваш код компилятором Flick,
собранным в WebAssembly.
