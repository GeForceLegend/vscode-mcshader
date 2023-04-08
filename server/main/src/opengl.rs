use std::ffi::{c_int, CStr, CString};
use std::os::raw::c_char;
use std::ptr;

pub struct OpenGlContext {
    _ctx: glutin::Context<glutin::PossiblyCurrent>,
}

impl OpenGlContext {
    pub fn new() -> OpenGlContext {
        let events_loop = glutin::event_loop::EventLoop::new();
        let not_current_context = glutin::ContextBuilder::new()
            .build_headless(&*events_loop, glutin::dpi::PhysicalSize::new(1, 1))
            .unwrap();

        let context = unsafe { not_current_context.make_current().unwrap() };
        gl::load_with(|symbol| context.get_proc_address(symbol).cast());

        OpenGlContext { _ctx: context }
    }

    pub fn validate_shader(&self, file_type: gl::types::GLenum, source: &str) -> Option<String> {
        unsafe {
            let shader = gl::CreateShader(file_type);
            let mut success = i32::from(gl::FALSE);
            let c_str_frag = CString::new(source).unwrap();
            gl::ShaderSource(shader, 1, &c_str_frag.as_ptr(), ptr::null());
            gl::CompileShader(shader);

            // Check for shader compilation errors
            gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut success);
            let result = if success != i32::from(gl::TRUE) {
                let mut info_len: gl::types::GLint = 0;
                gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut info_len);
                let mut info = Vec::with_capacity(info_len as usize);
                gl::GetShaderInfoLog(shader, info_len as c_int, ptr::null_mut(), info.as_mut_ptr() as *mut c_char);

                // ignore null for str::from_utf8
                info.set_len((info_len - 1) as usize);
                Some(String::from_utf8_unchecked(info))
            } else {
                None
            };
            gl::DeleteShader(shader);
            result
        }
    }

    pub fn vendor(&self) -> String {
        unsafe { String::from_utf8_unchecked(CStr::from_ptr(gl::GetString(gl::VENDOR) as *const _).to_bytes().to_vec()) }
    }
}

unsafe impl Sync for OpenGlContext {}

unsafe impl Send for OpenGlContext {}
