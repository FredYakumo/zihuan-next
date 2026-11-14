use salvo::prelude::*;
use salvo::writing::Json;
use serde::Serialize;

/// Identity of the running binary, stamped in by `build.rs`.
#[derive(Serialize)]
pub struct BuildInfo {
    pub version: &'static str,
    pub build_type: &'static str,
}

impl BuildInfo {
    pub fn current() -> Self {
        Self {
            version: env!("ZIHUAN_BUILD_VERSION"),
            build_type: env!("ZIHUAN_BUILD_TYPE"),
        }
    }
}

/// Reports the identity of the running binary so the admin UI can show which
/// build it is talking to.
#[handler]
pub async fn get_build_info(_req: &mut Request, res: &mut Response) {
    res.render(Json(BuildInfo::current()));
}
