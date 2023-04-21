# Change log

All notable changes to this vscode-mcshader will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)

## [Unreleased]

### Added

- A completely new double-linked file system, with less possible data racing and better edit experience;
- Tree-sitter based real-time syntax error linting;
- Tree-sitter based symbol provider (have some issues with macros, please keep the code as standard as possible);
- Edit include macros when renaming files or folders;

### Removed

- Removed compiling call in document_link function, compiling will only happen on file saving or disc content changing;

### Fixed

- Fixed errors of temp files keep in workspace error list after they are closed;
- Fixed deleting a workspace will not clean linting results of its contained files.

## [0.2.0]

### Added

- Single-file goto-definitions and references based on TreeSitter. This means this extension cannot find definitions or references outside of currently edited file right now.
- Notice: variables can only find definitions, but no references.

### Changed

- Optimized server initializing;
- Optimized file merging.

### Fixed

- Fixed crash while looking for shader pack path for temp files failed;
- Fixed file path with `../` or `./` in #include;
- Fixed issues of deleting a folder;
- Fixed linting if `#version` is not in the top line;
- Fixed a issue about linting errors across multiple files.

## [0.1.0] - 2023-02-20

### Added

- Real-time linting with optifine builtin macro support;
- Include document links;
- Multiple work space or multiple shader folders in one work space;
- Temporary linting and document link for files outside work space (temporary linting only supports base shader file);
- Virtual merge for base shader file;
- File watcher for file changes (creating, deleting, etc). Defaultly supports file with `[vsh, gsh, fsh, csh, glsl, inc]` extensions, you can add more by extension configuration.
