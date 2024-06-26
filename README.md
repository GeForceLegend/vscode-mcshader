# vscode-mcshader

vscode-mcshader is a new [Language Server](https://microsoft.github.io/language-server-protocol/) borns from [Strum355/mcshader-lsp](https://github.com/Strum355/mcshader-lsp/) with rewrited server side part, introducing lots of new LSP features that make your Minecraft shader developing experience better.

This extension only supports Windows platfrom currently. You can clone this repo and build it yourself if you want it running on other platforms.

## License

Part of code is released under the [MIT License]. Copyright (c) 2021 Noah Santschi-Cooney

Most code is released under the [MIT License]. Copyright (c) 2023 GeForceLegend

Work spaces support idea from Fayer3

## Features

 - Real-time linting with optifine builtin macro support;
 - Include document links;
 - Multiple work space or multiple shader folders in one work space;
 - Temporary linting and document link for files outside work space (temporary linting only supports base shader file);
 - Virtual merge for base shader file;
 - File watcher for file changes (creating, deleting, etc). Defaultly supports file with `[vsh, gsh, fsh, csh, glsl, inc]` extensions, you can add more by extension configuration;
 - Single-file goto-definitions and references;
 - Document symbols provider;
 - Workspace edits for include macro when renaming files.

This extension does not provide syntax highlight for GLSL yet. If you want GLSL syntax highlight, you can install this extension with [vscode-glsl](https://github.com/GeForceLegend/vscode-glsl) or [vscode-shader](https://github.com/stef-levesque/vscode-shader).

## Known issue

 - Code like this will disable inserted `#line` macro, and let the rest of this file reporting wrong line if error occured, unless found another active `#include`. To avoid this issue, please place an include that is always active behind it.
```glsl
#ifdef A // A is not defined defaultly
#include "B"
#endif

// To avoid this issue, please add an active include here before writing other code.
```

## Build and use guide

 - Run `cargo build --release` in `/server` (require rustc and cargo installed)
 - Move the build output `/server/target/release/vscode-mcshader(.exe)` to `/server`
 - Editing line 58 of `/client/src/extension.ts` to match the output. For Windows paltform, you can simply moving the build output `/server/target/release/vscode-mcshader(.exe)` to `/server`, while on other plaforms you may also need to change the `path.join()` params (as there is no `.exe` suffix)
 - Run `vsce package` in the root path (require npm and vsce installed)
 - Open your vscode and import the output vsix
