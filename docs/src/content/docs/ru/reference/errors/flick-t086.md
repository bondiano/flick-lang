---
title: "FLICK-T086"
source_hash: "0e65b71afa967e08"
---

# Функция строк блока, записанная вокруг них

В блоке, чьи строки — дети одного узла, например `ui { … }`, две и больше
строк уже дети одного `Ui.fragment`. `fragment` с детьми, записанными по
одному, вокруг них говорит то же самое.

```flick keep
view : Ui
let view = ui {
  fragment
    heading (text "count")
    paragraph (text "one")
}
```

это

```flick
view : Ui
let view = ui {
  heading (text "count")
  paragraph (text "one")
}
```

`flick check --fix --only T086` записывает детей строками блока: строки под
вызовом сдвигаются к его столбцу, а дети на его строке встают каждый на свою
строку или через `;` на строке `ui {`. Один ребёнок, `fragment x`, и один
список, `fragment rows`, остаются как есть.
