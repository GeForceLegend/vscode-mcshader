use hashbrown::HashSet;
use itoa::Buffer;
use lazy_static::lazy_static;
use regex::Regex;

use crate::diagnostics_parser::DiagnosticsParser;
use crate::opengl::OpenGlContext;

lazy_static! {
    pub static ref RE_DIMENSION_FOLDER: Regex = Regex::new(r#"^world-?\d+$"#).unwrap();
    pub static ref DEFAULT_SHADERS: HashSet<String> = {
        let mut set = HashSet::with_capacity(12064);
        for ext in ["fsh", "vsh", "gsh", "csh"] {
            set.insert("composite.".to_owned() + ext);
            set.insert("deferred.".to_owned() + ext);
            set.insert("prepare.".to_owned() + ext);
            set.insert("shadowcomp.".to_owned() + ext);
            for i in 1..=99 {
                let mut suffix = Buffer::new().format(i).to_owned() + ".";
                suffix += ext;
                set.insert("composite".to_owned() + &suffix);
                set.insert("deferred".to_owned() + &suffix);
                set.insert("prepare".to_owned() + &suffix);
                set.insert("shadowcomp".to_owned() + &suffix);
            }
            set.insert("final.".to_owned() + ext);
        }
        for ext in ["fsh", "vsh", "gsh"] {
            set.insert("gbuffers_armor_glint.".to_owned() + ext);
            set.insert("gbuffers_basic.".to_owned() + ext);
            set.insert("gbuffers_beaconbeam.".to_owned() + ext);
            set.insert("gbuffers_block.".to_owned() + ext);
            set.insert("gbuffers_clouds.".to_owned() + ext);
            set.insert("gbuffers_damagedblock.".to_owned() + ext);
            set.insert("gbuffers_entities.".to_owned() + ext);
            set.insert("gbuffers_entities_glowing.".to_owned() + ext);
            set.insert("gbuffers_hand.".to_owned() + ext);
            set.insert("gbuffers_hand_water.".to_owned() + ext);
            set.insert("gbuffers_line.".to_owned() + ext);
            set.insert("gbuffers_skybasic.".to_owned() + ext);
            set.insert("gbuffers_skytextured.".to_owned() + ext);
            set.insert("gbuffers_spidereyes.".to_owned() + ext);
            set.insert("gbuffers_terrain.".to_owned() + ext);
            set.insert("gbuffers_textured.".to_owned() + ext);
            set.insert("gbuffers_textured_lit.".to_owned() + ext);
            set.insert("gbuffers_water.".to_owned() + ext);
            set.insert("gbuffers_weather.".to_owned() + ext);
            set.insert("shadow.".to_owned() + ext);
        }
        let base_char_num = b'a';
        for suffix_num in 0u8..=25u8 {
            let suffix_char = unsafe { String::from_utf8_unchecked(vec![base_char_num + suffix_num, b'.', b'c', b's', b'h']) };
            set.insert("composite_".to_owned() + &suffix_char);
            set.insert("deferred_".to_owned() + &suffix_char);
            set.insert("prepare_".to_owned() + &suffix_char);
            set.insert("shadowcomp_".to_owned() + &suffix_char);
            for i in 1..=99 {
                let mut suffix = Buffer::new().format(i).to_owned() + "_";
                suffix += &suffix_char;
                set.insert("composite".to_owned() + &suffix);
                set.insert("deferred".to_owned() + &suffix);
                set.insert("prepare".to_owned() + &suffix);
                set.insert("shadowcomp".to_owned() + &suffix);
            }
        }
        set
    };
    pub static ref BASIC_EXTENSIONS: HashSet<String> = HashSet::from([
        "vsh".to_owned(),
        "gsh".to_owned(),
        "fsh".to_owned(),
        "csh".to_owned(),
        "glsl".to_owned(),
    ]);
    pub static ref RE_MACRO_CATCH: Regex = Regex::new(r#"(?m)^[ \f\t\v]*#(include|line).*$"#).unwrap();
    pub static ref RE_MACRO_INCLUDE: Regex = Regex::new(r#"^\s*#include\s+"(.+)""#).unwrap();
    pub static ref RE_MACRO_LINE: Regex = Regex::new(r#"^\s*#line"#).unwrap();
    pub static ref RE_MACRO_VERSION: Regex = Regex::new(r#"(?m)^[ \f\t\v]*#version.*$"#).unwrap();
    pub static ref RE_MACRO_INCLUDE_MULTI_LINE: Regex = Regex::new(r#"(?m)^[ \f\t\v]*#include\s+"(.+)".*$"#).unwrap();
    pub static ref OPENGL_CONTEXT: OpenGlContext = OpenGlContext::new();
    pub static ref DIAGNOSTICS_PARSER: DiagnosticsParser = DiagnosticsParser::new(&OPENGL_CONTEXT);
}

pub const OPTIFINE_MACROS: &str = "#define MC_VERSION 11900
#define MC_GL_VERSION 320
#define MC_GLSL_VERSION 150
#define MC_OS_WINDOWS
#define MC_GL_VENDOR_NVIDIA
#define MC_GL_RENDERER_GEFORCE
#define MC_NORMAL_MAP
#define MC_SPECULAR_MAP
#define MC_RENDER_QUALITY 1.0
#define MC_SHADOW_QUALITY 1.0
#define MC_HAND_DEPTH 0.125
#define MC_RENDER_STAGE_NONE 0
#define MC_RENDER_STAGE_SKY 1
#define MC_RENDER_STAGE_SUNSET 2
#define MC_RENDER_STAGE_SUN 4
#define MC_RENDER_STAGE_CUSTOM_SKY 3
#define MC_RENDER_STAGE_MOON 5
#define MC_RENDER_STAGE_STARS 6
#define MC_RENDER_STAGE_VOID 7
#define MC_RENDER_STAGE_TERRAIN_SOLID 8
#define MC_RENDER_STAGE_TERRAIN_CUTOUT_MIPPED 9
#define MC_RENDER_STAGE_TERRAIN_CUTOUT 10
#define MC_RENDER_STAGE_ENTITIES 11
#define MC_RENDER_STAGE_BLOCK_ENTITIES 12
#define MC_RENDER_STAGE_DESTROY 13
#define MC_RENDER_STAGE_OUTLINE 14
#define MC_RENDER_STAGE_DEBUG 15
#define MC_RENDER_STAGE_HAND_SOLID 16
#define MC_RENDER_STAGE_TERRAIN_TRANSLUCENT 17
#define MC_RENDER_STAGE_TRIPWIRE 18
#define MC_RENDER_STAGE_PARTICLES 19
#define MC_RENDER_STAGE_CLOUDS 20
#define MC_RENDER_STAGE_RAIN_SNOW 21
#define MC_RENDER_STAGE_WORLD_BORDER 22
#define MC_RENDER_STAGE_HAND_TRANSLUCENT 23
";
