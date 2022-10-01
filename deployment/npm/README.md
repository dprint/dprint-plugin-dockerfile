# @dprint/dockerfile

npm distribution of [dprint-plugin-dockerfile](https://github.com/dprint/dprint-plugin-dockerfile).

Use this with [@dprint/formatter](https://github.com/dprint/js-formatter) or just use @dprint/formatter and download the [dprint-plugin-dockerfile Wasm file](https://github.com/dprint/dprint-plugin-dockerfile/releases).

## Example

```ts
import { getPath } from "@dprint/dockerfile";
import { createFromBuffer } from "@dprint/formatter";
import * as fs from "fs";

const buffer = fs.readFileSync(getPath());
const formatter = createFromBuffer(buffer);

console.log(formatter.formatText("test.dockerfile", "RUN      /bin/bash"));
```
