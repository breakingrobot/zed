//! Times how long Zed takes to create and to reopen a real dev container, to
//! compare with the reference CLI (`devcontainer up`) on the same project:
//!
//! ```sh
//! ZED_DEV_CONTAINER_BENCH=/path/to/project cargo test -p dev_container \
//!     bench::time_dev_container_starts -- --ignored --nocapture
//! ```

use std::{collections::HashMap, path::Path, sync::Arc, time::Instant};

use gpui::{TestAppContext, WeakEntity};
use remote::EngineHost;

use crate::{
    DevContainerContext, SessionCache,
    devcontainer_api::{BuildMode, DevContainerConfig},
    devcontainer_manifest::spawn_dev_container,
};

#[gpui::test]
#[ignore = "needs Docker and a project in ZED_DEV_CONTAINER_BENCH"]
async fn time_dev_container_starts(cx: &mut TestAppContext) {
    let Ok(project) = std::env::var("ZED_DEV_CONTAINER_BENCH") else {
        return;
    };
    // The engine runs real processes, which the deterministic executor can't drive.
    cx.executor().allow_parking();
    let project_directory: Arc<Path> = Path::new(&project).into();
    let context = DevContainerContext {
        project_directory: project_directory.clone(),
        engine_host: EngineHost::Local,
        use_podman: false,
        use_buildkit: None,
        dotfiles: None,
        secrets_file: None,
        workspace_volume: None,
        remote_engine: false,
        session_cache: SessionCache::default(),
        fs: fs::RealFs::new(None, cx.executor()),
        http_client: Arc::new(reqwest_client::ReqwestClient::new()),
        environment: WeakEntity::new_invalid(),
    };
    for (label, build_mode) in [
        ("create", BuildMode::Rebuild),
        ("reopen running", BuildMode::Reuse),
    ] {
        let started = Instant::now();
        spawn_dev_container(
            &context,
            HashMap::default(),
            DevContainerConfig::default_config(),
            &project_directory,
            build_mode,
            false,
        )
        .await
        .unwrap_or_else(|error| panic!("{label}: {error}"));
        println!("{label}: {:.1}s", started.elapsed().as_secs_f64());
    }
}
