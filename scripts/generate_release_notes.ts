import { generateChangeLog } from "jsr:@dprint/automation@0.12.2";

const version = Deno.args[0];
const changelog = await generateChangeLog({
  versionTo: version,
});
const text = `## Changes

${changelog}

## Install

[Install](https://dprint.dev/install/) and [setup](https://dprint.dev/setup/) dprint.

Then in your project's dprint configuration file:

1. Run \`dprint add dockerfile\` or specify the plugin url in the \`"plugins"\` array.
2. Add a \`"dockerfile"\` configuration property if desired.
   \`\`\`jsonc
   {
     // ...etc...
     "dockerfile": {
       // config goes here
     },
     "plugins": [
       "npm:@dprint/dockerfile@${version}"
     ]
   }
   \`\`\`

## JS Formatting API

* [JS Formatter](https://github.com/dprint/js-formatter) - Browser/Deno and Node
* [npm package](https://www.npmjs.com/package/@dprint/dockerfile)
`;

console.log(text);
