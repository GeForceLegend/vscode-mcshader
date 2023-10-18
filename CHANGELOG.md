# Change log

All notable changes to this vscode-mcshader will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)

## [Unreleased]

### Fixed

- Fixed some invalid shader name like `composite0` and `composite0X` will considered as shader files.
- Fixed [vscode-languageserver-node/#1215](https://github.com/microsoft/vscode-languageserver-node/issues/1215) in previous versions by updating `vscode-languageclient`.

## [0.4.5] 2023-10-12

### Fixed

- Fixed wrong text sync when editing end of the file.

## [0.4.4] 2023-10-09

### Fixed

- Fixed Optifine builtin macro is not inserted if there is no `#version` macro.
- Fixed some issues about file deleting.
- Fixed possible vec capacity overflow when parsing compile log.

## [0.4.3] 2023-10-07

### Changed

- Deleted file property edits in renaming files. Renaming file function will just return the workspace edits, and renamed files will be handled in `update_watched_files()`.

### Fixed

- Fixed possible crash when a file is included in its including tree.

## [0.4.2] 2023-10-02

### Changed

- Updated `tree-sitter-glsl` to 0.1.5

### Fixed

- Fixed possible issue with document symbles without a name.
- Fixed possible crash when a file is included multiple times in one shader.

## [0.4.1] 2023-09-26

### Fixed

- Fixed a possible issue that makes some pointer point to unexpected memory.

## [0.4.0] 2023-09-24

### Changed

- Ignore folder starts with `.` on initializing, excepting `.minecraft`. This intends to ignore developing environment content like git.

### Fixed

- Fixed wrong linting result that may occured if a temp file's incluide file exists but unable to read.
- Fixed shader pack path not removed when deleting workspace.

## [0.3.6] - 2023-09-10

### Fixed

- Fixed server crash while editing line with nothing.

## [0.3.5] - 2023-09-10

### Changed

- Optimized workspace scanning.

### Fixed

- Fixed delete a file cleans its related shader list, but it will not filled with proper data when file comes back.
- Fixed including file might created from a folder but not a file.
- Fixed possible crash while saving a file without extension.

## [0.3.4] - 2023-07-16

### Changed

- Optimized file merging.

### Fixed

- Fixed line offset in different GLSL versions. Tested on NVIDIA driver 536.23 and AMD driver 23.7.1, may be different in other driver versions or other platforms.
- Fixed function `update_shader_list` does not really updates parent_shaders, it only deletes the diagnostics from deleted parent shaders.

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
- Fixed file path with `../` or `./` in `#include`;
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
