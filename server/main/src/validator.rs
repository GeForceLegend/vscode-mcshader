use glslang::{error::GlslangError, *};

pub struct ShaderCompiler {
    compiler: &'static Compiler,
    options: CompilerOptions,
}

impl ShaderCompiler {
    pub fn new() -> Self {
        let compiler = Compiler::acquire().unwrap();
        let options = CompilerOptions {
            target: Target::None(None),
            ..Default::default()
        };
        Self { compiler, options }
    }

    pub fn validate(&self, source: &str, shater_type: ShaderStage) -> Option<String> {
        let source = ShaderSource::try_from(source).unwrap();
        let input = ShaderInput::new(&source, shater_type, &self.options, None).unwrap();
        match Shader::new(self.compiler, input) {
            Ok(_) => None,
            Err(GlslangError::ParseError(err)) => Some(err),
            // This will never be used, as glslang-rs crate only returns ParseError when error occured
            Err(_) => None,
        }
    }
}
