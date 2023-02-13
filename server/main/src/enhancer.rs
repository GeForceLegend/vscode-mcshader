use std::path::PathBuf;

use path_slash::PathBufExt;
use url::Url;

use fluent_uri::{enc, Uri};

pub trait FromUrl {
    fn from_url(u: Url) -> Self;
}

impl FromUrl for PathBuf {
    #[cfg(target_family = "windows")]
    fn from_url(u: Url) -> Self {
        let path = percent_encoding::percent_decode_str(u.path().strip_prefix('/').unwrap())
            .decode_utf8()
            .unwrap();

        PathBuf::from_slash(path)
    }

    #[cfg(target_family = "unix")]
    fn from_url(u: Url) -> Self {
        let path = percent_encoding::percent_decode_str(u.path()).decode_utf8().unwrap();
        
        trace!("converted unix path from url"; "old" => u.as_str(), "new" => path.to_string());

        PathBuf::from_slash(path)
    }
}

pub trait FromUri {
    fn from_uri(u: String) -> Self;
}

impl FromUri for PathBuf {
    #[cfg(target_family = "windows")]
    fn from_uri(u: String) -> Self {
        use std::path;

        let path = Uri::parse(&u).unwrap().path();
        let path = enc::EStr::new(path.as_str()).decode().into_string().unwrap().to_string();
        let path = path.strip_prefix('/').unwrap();
        let path = path.replace("/", path::MAIN_SEPARATOR_STR);

        PathBuf::from(path)
    }
}
