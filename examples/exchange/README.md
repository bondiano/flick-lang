# An order book that traders fill at once

A limit order book with a matching engine. Several traders send their orders
at the same time, as JSON lines over one queue. A sequencer puts the lines
back in order and submits them to the exchange. The result of a session is
the same on every run and on both targets, so the example also works as a
benchmark and as a test of the backends: `tools/bench` times it on Node and
as a Go executable and requires both to print the same report.

| Part | Where |
| - | - |
| orders, commands and events: variants, a record payload, `deriving Schema` | `src/Book/Order.fl` |
| price-time priority: a `PriorityQueue` a side, bids under an `Ord` that puts the highest first | `src/Book/Book.fl` |
| cancels without a search: a `HashMap` of live orders, dead ones dropped when they reach the top | `src/Book/Book.fl` |
| a command for each number, the same on every target, and its JSON line; a bad line is the typed error `Malformed` | `src/Book/Feed.fl` |
| the exchange as a `service`, given by a `Layer` over a `Ref` | `src/App/Exchange.fl` |
| traders and a sequencer: `parallel`, `parallel_all`, a bounded `Queue`, a second `PriorityQueue` that restores the order | `src/App/Session.fl` |
| the report and the screens of `watch`, ANSI escapes only | `src/App/View.fl` |
| the command line: a `cli` block with `run` and `watch`, the flags checked | `src/App/Main.fl` |

## Run it from the repository root

Use the checkout's [source setup](../../README.md#try-it-from-source), including
the pinned Rust toolchain, Node.js and pnpm:

```sh
pnpm install --frozen-lockfile
pnpm -C runtime build
pnpm -C packages/effect-ts build
cargo build -p flick

./target/debug/flick run --project examples/exchange -- run --orders 5000
./target/debug/flick run --project examples/exchange -- watch
./target/debug/flick run --project examples/exchange -- --help
./target/debug/flick test --project examples/exchange
./target/debug/flick test --project examples/exchange --target go
```

If `flick` is already on your `PATH`, replace `./target/debug/flick` with `flick`.
`run --orders 5000` prints the report in [`expected.stdout`](expected.stdout):

```text
5000 commands from 4 traders
trades 3263, volume 34116
rested 2557, dropped 1, cancelled 326, refused 473, malformed 0
checksum 1249589472
best bid 1047, best ask 1050
resting 418 bids, 108 asks
```

The checksum folds every event in the order the book produced them. Two
runs with the same checksum almost certainly gave the same answers to the
same commands: it is a 32-bit hash, not a proof.
`--traders` changes how many traders send the commands, and only the
first line of the report.

`watch` runs a smaller session and redraws the book in place every
`--every` commands: the progress, the tally and the best eight price levels
of each side, the asks in red above the spread and the bids in green below
it. `--pause` sets the wait after each redraw, in milliseconds.

## Price-time priority

The book keeps one `PriorityQueue` a side. The queue releases the least
priority first and, at equal priorities, the pair pushed first. For asks,
that is the lowest price and then the earliest order. For bids, it is the
same rule with the order of prices reversed:

```flick
type BidPrice = BidPrice Int
  deriving Eq

instance Ord BidPrice =
  let compare (BidPrice a) (BidPrice b) = compare b a
```

A priority queue can only take out its first pair. A cancel or a fill
removes the order from the `live` map. A dead order stays in its queue until
it reaches the top, where the matching loop drops it. A partial fill keeps
the order at the top and lowers its quantity in `live`. Each order rests
under a ticket of its own, which `live` keeps with its quantity, so an id
that is placed again after its order is gone does not bring back the old
entry.

## Why the session is deterministic

`Book.Feed.command seq` is a pure function of the command's number. It
mixes the bits of `seq` with the finalizer of MurmurHash3 on the 32-bit
wrapping `Int`, which wraps the same way on both targets. Trader `t` sends
the commands numbered `t` modulo the number of traders. The queue receives
the lines in whatever order the scheduler gives. The sequencer holds each
line in a `PriorityQueue` by number and submits it only after every line
before it, so the book sees the commands in order. The session tests check
this with 1, 3 and 7 traders against the same commands replayed one by one.

## Tests

- `test/Book/BookTest.fl`: a case for each rule, with `test.each`.
  - A limit order rests when it does not cross.
  - A fill is at the resting order's price, and a better price fills first.
  - Equal prices fill in arrival order.
  - A market order drops what it cannot fill.
  - A cancel reports what was left.
  - A filled or unknown order cannot be cancelled.
  - An id placed again after its order is gone rests at its new price and time.
  - An order with no quantity, a limit price that is not positive, or the id
    of a resting order is refused.
- `test/Book/PropertyTest.fl`: properties on generated streams of up to 60
  commands, over few prices and small quantities:
  - each step gives the same events and leaves the same book as the naive book of
    `test/Book/Reference.fl`, which scans sorted lists;
  - the best bid stays below the best ask;
  - every fill is at the resting price and within the taker's limit;
  - every unit of every order filled, dropped, was cancelled or still rests.
- `test/App/SessionTest.fl`:
  - a concurrent session equals the commands replayed in order;
  - every command survives its JSON line;
  - a bad line fails with `Malformed`;
  - the ladder puts the asks above the spread and the bids below it.

## As a benchmark

```sh
just bench
```

`tools/bench/index.mjs` builds the example in release for both targets and
times `run`, 200 000 commands by default, beside the single-file programs.
The row measures the whole program: encoding and decoding JSON, the queue
and fibers, and the persistent heap and map of the book.
