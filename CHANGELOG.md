# Change log

All notable changes to this vscode-mcshader will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)

## [Unreleased]

### Changed

- Optimized file merging.

### Fixed

- Fixed line offset in different GLSL versions. Tested on NVIDIA driver 536.23 and AMD driver 23.7.1, may be different in other driver versions or other platforms.

## [0.3.3] - 2023-05-29

### Fixed

- Fixed non-ASCII characters (take 2 or more u8 in String) parsing.

## [0.3.2] - 2023-05-26

### Added

- Added an option to enable or disable temp file linting using tree-sitter-glsl. Defaultly off because I found it sucks.

### Fixed

- Fixed a multiple file linting issue: If file A and B both includes file C, and linting B will show a error in C, when linting A again, error caused by B will disappear until linting B again.
- Fixed a multiple file linting issue: If file A includes B, and linting A will show a error in B, when deleting B from A's includes, errors in B will not disappear.

## [0.3.1] - 2023-05-04

### Fixed

- Fixed file ID when merging shader files.

## [0.3.0] - 2023-04-25

### Added

- A completely new double-linked file system, with less possible data racing and better edit experience;
- Tree-sitter based symbol provider (have some issues with macros, please keep the code as standard as possible);
- Apply edits to include macros when renaming files or folders (because of [vscode-languageserver-node/#1215](https://github.com/microsoft/vscode-languageserver-node/issues/1215), server cannot receive rename request from pathes that contain `.minecraft`, please develop outside the game folder or using `mklink` command line to provide a path without `.minecraft` to use this feature);

### Fixed

- Fixed errors of temp files keep in workspace error list after they are closed;
- Fixed deleting a workspace will not clean linting results of its contained files.

## [0.2.0] - 2023-04-11

### Added

- Single-file goto-definitions and references based on TreeSitter. This means this extension cannot find definitions or references outside of currently edited file right now.
- Notice: variables can only find definitions, but no references.

### Changed

- Optimized server initializing;
- Optimized file merging.

### Fixed

- Fixed crash while searching shader pack path for temp files failed;
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
