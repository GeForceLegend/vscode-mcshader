use logging::warn;

use super::*;

impl TempFile {
    pub fn new(parser: &mut Parser, file_path: &PathBuf) -> Option<Self> {
        warn!("Document not found in file system"; "path" => file_path.to_str().unwrap());
        let content = match read_to_string(file_path) {
            Ok(content) => RefCell::from(content),
            Err(_err) => return None,
        };
        let file_type = match file_path.extension() {
            Some(extension) => {
                if extension == "fsh" {
                    gl::FRAGMENT_SHADER
                } else if extension == "vsh" {
                    gl::VERTEX_SHADER
                } else if extension == "gsh" {
                    gl::GEOMETRY_SHADER
                } else if extension == "csh" {
                    gl::COMPUTE_SHADER
                } else {
                    gl::NONE
                }
            }
            None => gl::NONE,
        };
        let mut buffer = file_path.components();
        loop {
            match buffer.next_back() {
                Some(Component::Normal(file_name)) => {
                    if file_name == "shaders" {
                        break;
                    } else {
                        continue;
                    }
                }
                _ => return None,
            }
        }

        let mut resource = OsString::new();
        for component in buffer {
            resource.push(component);
            match component {
                Component::Prefix(_) | Component::RootDir => {}
                _ => resource.push(MAIN_SEPARATOR_STR),
            }
        }
        resource.push("shaders");

        Some(TempFile {
            content,
            file_type,
            pack_path: PathBuf::from(resource),
            tree: RefCell::from(parser.parse("", None).unwrap()),
        })
    }

    pub fn update_self(&mut self, file_path: &PathBuf) {
        *self.content.borrow_mut() = match read_to_string(file_path) {
            Ok(content) => content,
            Err(_err) => String::new(),
        };
    }

    pub fn merge_self(&self, file_path: &PathBuf, file_list: &mut HashMap<String, PathBuf>) -> Option<(u32, String)> {
        if self.file_type == gl::NONE {
            return None;
        }

        let mut temp_content = String::new();
        file_list.insert("0".to_owned(), file_path.clone());
        let mut file_id = 0;
        let file_name = file_path.to_str().unwrap();

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

                let include_path = match path.strip_prefix('/') {
                    Some(path) => self.pack_path.join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                    None => file_path
                        .parent()
                        .unwrap()
                        .join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                };
                temp_content += before_content;
                start_index = end;
                lines += before_content.matches("\n").count();

                Self::merge_temp(&self.pack_path, include_path, file_list, capture_content, &mut temp_content, &mut file_id, 1);
                temp_content += &generate_line_macro(lines, "0", file_name);
            } else if RE_MACRO_LINE.is_match(capture_content) {
                temp_content += before_content;
                start_index = end;
                lines += before_content.matches("\n").count();
            }
        });
        temp_content += unsafe { content.get_unchecked(start_index..) };

        // Move #version to the top line
        if let Some(capture) = RE_MACRO_VERSION.captures(&temp_content) {
            let version = capture.get(0).unwrap();
            let mut version_content = version.as_str().to_owned() + "\n";

            temp_content.replace_range(version.start()..version.end(), "");
            // If we are not in the debug folder, add Optifine's macros
            if self.pack_path.parent().unwrap().file_name().unwrap() != "debug" {
                version_content += OPTIFINE_MACROS;
            }
            version_content += "#line 1 0\t//";
            version_content += file_name;
            version_content += "\n";
            temp_content.insert_str(0, &version_content);
        }

        Some((self.file_type, temp_content))
    }

    fn merge_temp(
        pack_path: &PathBuf, file_path: PathBuf, file_list: &mut HashMap<String, PathBuf>, original_content: &str,
        temp_content: &mut String, file_id: &mut i32, depth: i32,
    ) {
        if depth > 10 || !file_path.exists() {
            // If include depth reaches 10 or file does not exist
            // Leave the include alone for reporting a error
            temp_content.push_str(original_content);
            temp_content.push('\n');
            return;
        }
        *file_id += 1;
        let curr_file_id = Buffer::new().format(*file_id).to_owned();
        let file_name = file_path.to_str().unwrap();
        temp_content.push_str(&generate_line_macro(1, &curr_file_id, file_name));
        temp_content.push('\n');

        if let Ok(content) = read_to_string(&file_path) {
            let mut start_index = 0;
            let mut lines = 2;

            RE_MACRO_CATCH.find_iter(content.as_ref()).for_each(|macro_line| {
                let start = macro_line.start();
                let end = macro_line.end();

                let before_content = unsafe { content.get_unchecked(start_index..start) };
                let capture_content = macro_line.as_str();
                if let Some(capture) = RE_MACRO_INCLUDE.captures(capture_content) {
                    let path = capture.get(1).unwrap().as_str();

                    let include_path = match path.strip_prefix('/') {
                        Some(path) => pack_path.join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                        None => file_path
                            .parent()
                            .unwrap()
                            .join(PathBuf::from(path.replace("/", MAIN_SEPARATOR_STR))),
                    };
                    temp_content.push_str(before_content);
                    start_index = end;
                    lines += before_content.matches("\n").count();

                    Self::merge_temp(pack_path, include_path, file_list, capture_content, temp_content, file_id, depth + 1);
                    temp_content.push_str(&generate_line_macro(lines, &curr_file_id, file_name));
                } else if RE_MACRO_LINE.is_match(capture_content) {
                    temp_content.push_str(before_content);
                    start_index = end;
                    lines += before_content.matches("\n").count();
                }
            });
            temp_content.push_str(unsafe { content.get_unchecked(start_index..) });
            temp_content.push('\n');
            file_list.insert(curr_file_id, file_path);
        } else {
            warn!("Unable to read file"; "path" => file_path.to_str().unwrap());
            temp_content.push_str(original_content);
            temp_content.push('\n');
        }
    }
}

impl File for TempFile {
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
