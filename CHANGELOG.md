# Change log

All notable changes to this vscode-mcshader will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)

## [Unreleased]

### Added

- Single-file goto-definition and references based on TreeSitter. This means this extension cannot find definitions or references outside of currently edited file right now.

### Fixed

- Fixed crash while looking for shader pack path for temp files failed;
- Fixed file path with `../` in #include;
- Fixed deleting a folder will not delete files in server data;
- Fixed linting if `#version` is not in the top line.

## [0.1.0] - 2023-02-20

### Added

- Real-time linting with optifine builtin macro support;
- Include document links;
- Multiple work space or multiple shader folders in one work space;
- Temporary linting and document link for files outside work space (temporary linting only supports base shader file);
- Virtual merge for base shader file;
- File watcher for file changes (creating, deleting, etc). Defaultly supports file with `[vsh, gsh, fsh, csh, glsl, inc]` extensions, you can add more by extension configuration.
