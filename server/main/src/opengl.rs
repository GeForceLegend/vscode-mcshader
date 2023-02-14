use std::ffi::{CStr, CString};
use std::ptr;

pub struct OpenGlContext {
    _ctx: glutin::Context<glutin::PossiblyCurrent>,
}

impl OpenGlContext {
    pub fn new() -> OpenGlContext {
        let events_loop = glutin::event_loop::EventLoop::new();
        let gl_window = glutin::ContextBuilder::new()
            .build_headless(&*events_loop, glutin::dpi::PhysicalSize::new(1, 1))
            .unwrap();

        let gl_window = unsafe {
            let gl_window = gl_window.make_current().unwrap();
            gl::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _);
            gl_window
        };

        OpenGlContext { _ctx: gl_window }
    }

    unsafe fn compile_and_get_shader_log(&self, shader: gl::types::GLuint, source: &str) -> Option<String> {
        let mut success = i32::from(gl::FALSE);
        let c_str_frag = CString::new(source).unwrap();
        gl::ShaderSource(shader, 1, &c_str_frag.as_ptr(), ptr::null());
        gl::CompileShader(shader);

        // Check for shader compilation errors
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut success);
        let result = if success != i32::from(gl::TRUE) {
            let mut info_len: gl::types::GLint = 0;
            gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut info_len);
            let mut info = vec![0u8; info_len as usize];
            gl::GetShaderInfoLog(
                shader,
                info_len as gl::types::GLsizei,
                ptr::null_mut(),
                info.as_mut_ptr() as *mut gl::types::GLchar,
            );
            info.set_len((info_len - 1) as usize); // ignore null for str::from_utf8
            Some(String::from_utf8(info).unwrap())
        } else {
            None
        };
        gl::DeleteShader(shader);
        result
    }

    pub fn validate_shader(&self, file_type: &gl::types::GLenum, source: &str) -> Option<String> {
        unsafe {
            let shader = gl::CreateShader(file_type.clone());
            self.compile_and_get_shader_log(shader, source)
        }
    }

    pub fn vendor(&self) -> String {
        unsafe { String::from_utf8(CStr::from_ptr(gl::GetString(gl::VENDOR) as *const _).to_bytes().to_vec()).unwrap() }
    }
}
