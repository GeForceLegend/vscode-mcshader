use super::*;

impl IncludeFile {
    pub fn included_shaders(&self) -> &RefCell<HashSet<PathBuf>> {
        &self.included_shaders
    }

    pub fn including_files(&self) -> &RefCell<HashSet<PathBuf>> {
        &self.including_files
    }

    pub fn parent_update_list(&self, include_files: &HashMap<PathBuf, IncludeFile>, update_list: &mut HashSet<PathBuf>, depth: i32) {
        if depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return;
        }
        // Insert files that need to update parents into a list
        for file in self.including_files.borrow().iter() {
            if let Some(include_file) = include_files.get(file) {
                update_list.insert(file.clone());
                include_file.parent_update_list(include_files, update_list, depth + 1);
            }
        }
    }

    pub fn get_includes(
        include_files: &mut HashMap<PathBuf, IncludeFile>, parent_update_list: &mut HashSet<PathBuf>, parser: &mut Parser,
        pack_path: &PathBuf, include_path: PathBuf, parent_file: &HashSet<PathBuf>, depth: i32,
    ) {
        if !include_path.exists() || depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            return;
        } else if let Some(include_file) = include_files.get(&include_path) {
            // Insert all include files that need to update parent shader to a list
            // And add parent shader together
            parent_update_list.insert(include_path);
            include_file.parent_update_list(include_files, parent_update_list, depth + 1);
        } else {
            if let Ok(content) = read_to_string(&include_path) {
                let mut including_files = HashSet::new();
                RE_MACRO_INCLUDE_MULTI_LINE.captures_iter(&content).for_each(|captures| {
                    let path = captures.get(1).unwrap().as_str();

                    match include_path_join(pack_path, &include_path, path) {
                        Ok(sub_include_path) => {
                            including_files.insert(sub_include_path.clone());
                            Self::get_includes(
                                include_files,
                                parent_update_list,
                                parser,
                                pack_path,
                                sub_include_path,
                                parent_file,
                                depth + 1,
                            );
                        }
                        Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
                    }
                });
                let include_file = IncludeFile {
                    tree: RefCell::from(parser.parse(&content, None).unwrap()),
                    content: RefCell::from(content),
                    pack_path: pack_path.clone(),
                    included_shaders: RefCell::from(parent_file.clone()),
                    including_files: RefCell::from(including_files),
                };
                include_files.insert(include_path, include_file);
            } else {
                error!("Unable to read file {}", include_path.to_str().unwrap());
            }
        }
    }

    pub fn update_include(&self, include_files: &mut HashMap<PathBuf, IncludeFile>, parser: &mut Parser, file_path: &PathBuf) {
        let mut including_files = self.including_files.borrow_mut();
        including_files.clear();

        if let Ok(content) = read_to_string(file_path) {
            let mut parent_update_list: HashSet<PathBuf> = HashSet::new();
            let included_shaders = self.included_shaders.borrow();
            RE_MACRO_INCLUDE_MULTI_LINE.captures_iter(&content).for_each(|captures| {
                let path = captures.get(1).unwrap().as_str();

                match include_path_join(&self.pack_path, file_path, path) {
                    Ok(sub_include_path) => {
                        including_files.insert(sub_include_path.clone());
                        Self::get_includes(
                            include_files,
                            &mut parent_update_list,
                            parser,
                            &self.pack_path,
                            sub_include_path,
                            &included_shaders,
                            1,
                        );
                    }
                    Err(error) => error!("Unable to parse include link {}, error: {}", path, error),
                }
            });
            for include_path in parent_update_list {
                include_files
                    .get(&include_path)
                    .unwrap()
                    .included_shaders
                    .borrow_mut()
                    .extend(included_shaders.clone());
            }
            *self.tree.borrow_mut() = parser.parse(&content, None).unwrap();
            *self.content.borrow_mut() = content;
        } else {
            error!("Unable to read file"; "path" => file_path.to_str().unwrap());
        }
    }

    pub fn merge_include(
        &self, include_files: &HashMap<PathBuf, IncludeFile>, file_path: PathBuf, original_content: &str,
        file_list: &mut HashMap<String, PathBuf>, shader_content: &mut String, file_id: &mut i32, depth: i32,
    ) {
        if !file_path.exists() || depth > 10 {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            // return original_content.to_owned() + "\n";
            shader_content.push_str(original_content);
            shader_content.push('\n');
            return;
        }
        *file_id += 1;
        let curr_file_id = Buffer::new().format(*file_id).to_owned();
        let file_name = file_path.to_str().unwrap();
        shader_content.push_str(&generate_line_macro(1, &curr_file_id, file_name));
        shader_content.push('\n');

        let content = self.content.borrow();
        let mut start_index = 0;
        let mut lines = 2;

        RE_MACRO_CATCH.find_iter(content.as_ref()).for_each(|macro_line| {
            let start = macro_line.start();
            let end = macro_line.end();

            let before_content = unsafe { content.get_unchecked(start_index..start) };
            let capture_content = macro_line.as_str();
            if let Some(capture) = RE_MACRO_INCLUDE.captures(capture_content) {
                let path = capture.get(1).unwrap().as_str();

                if let Ok(include_path) = include_path_join(&self.pack_path, &file_path, path) {
                    if let Some(include_file) = include_files.get(&include_path) {
                        shader_content.push_str(before_content);
                        start_index = end;
                        lines += before_content.matches("\n").count();

                        include_file.merge_include(include_files, include_path, capture_content, file_list, shader_content, file_id, depth + 1);
                        shader_content.push_str(&generate_line_macro(lines, &curr_file_id, file_name));
                    }
                }
            } else if RE_MACRO_LINE.is_match(capture_content) {
                shader_content.push_str(before_content);
                start_index = end;
                lines += before_content.matches("\n").count();
            }
        });
        shader_content.push_str(unsafe { content.get_unchecked(start_index..) });
        shader_content.push('\n');
        file_list.insert(curr_file_id, file_path);
    }
}

impl File for IncludeFile {
    fn pack_path(&self) -> &PathBuf {
        &self.pack_path
    }

    fn content(&self) -> &RefCell<String> {
        &self.content
    }

    fn tree(&self) -> &RefCell<Tree> {
        &self.tree
    }
}
