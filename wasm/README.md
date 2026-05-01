# nixfmt-rs (WebAssembly)

Format Nix code from JavaScript/TypeScript. Same output as
[`nixfmt`](https://github.com/NixOS/nixfmt), byte-for-byte.

```sh
npm install nixfmt-rs
```

## Node / bundlers

```ts
import { format, version } from "nixfmt-rs";

format("{a=1;}");                          // "{ a = 1; }\n"
format(src, { width: 80, indent: 2, filename: "default.nix" });
version();                                 // "0.1.2"
```

Parse failures throw a `ParseError` with `message` (one line),
`diagnostic` (rendered snippet) and `range: [start, end]` byte offsets.

## Browser (no bundler)

```js
import init, { format } from "nixfmt-rs/web";
await init();
format("{a=1;}");
```

See <https://mic92.github.io/nixfmt-rs/> for a live playground.
