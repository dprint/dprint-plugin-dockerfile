# @dprint/dockerfile

npm distribution of [dprint-plugin-dockerfile](https://github.com/dprint/dprint-plugin-dockerfile).

Use this with [@dprint/formatter](https://github.com/dprint/js-formatter) or just use @dprint/formatter and download the [dprint-plugin-dockerfile WASM file](https://github.com/dprint/dprint-plugin-dockerfile/releases).

## Example

```ts
import { getBuffer } from "@dprint/dockerfile";
import { createFromBuffer } from "@dprint/formatter";

const formatter = createFromBuffer(getBuffer());

console.log(formatter.formatText("test.dockerfile", "RUN      /bin/bash"));
```
