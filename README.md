# vscode-mcshader

vscode-mcshader is a new [Language Server](https://microsoft.github.io/language-server-protocol/) based on [Strum355/mcshader-lsp](https://github.com/Strum355/mcshader-lsp/), but rewrites the server with [tower-lsp](https://github.com/ebkalderon/tower-lsp). This extension is WIP and lacks many features that already existing in Strum355/mcshader-lsp.

If you are interested in what I have done before rewrite it with tower-lsp, you can find them in [GeForceLegend/mcshader-lsp/file-system-rewrite](https://github.com/GeForceLegend/mcshader-lsp/tree/file-system-rewrite).

This extension only supports Windows platfrom currently. You can clone this repo and build it yourself if you want it running on other platforms.

## License

Client and some code in server is released under the [MIT License]. Copyright (c) 2021 Noah Santschi-Cooney

Most server code is released under the [MIT License]. Copyright (c) 2023 GeForceLegend

Work spaces support idea from Fayer3

## Features

 - Real-time linting with optifine builtin macro support;
 - Include document links;
 - Multiple work space or multiple shader folders in one work space;
 - Temporary linting and document link for files outside work space (temporary linting only supports base shader file);
 - Virtual merge for base shader file;
 - File watcher for file changes (creating, deleting, etc). Defaultly supports file with `[vsh, gsh, fsh, csh, glsl, inc]` extensions, you can add more by extension configuration.

This extension does not provide syntax highlight for GLSL yet. If you want GLSL syntax highlight, you can install this extension with [vscode-glsl](https://github.com/GeForceLegend/vscode-glsl) or [vscode-shader](https://github.com/stef-levesque/vscode-shader).
