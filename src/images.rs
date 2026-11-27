//! `sarda prepare` and `sarda load`: getting the two images onto a host that
//! cannot pull them at run time. Both only drive the container [`Runtime`].

use crate::cli::{LoadArgs, PrepareArgs};
use crate::error::{Error, Kind, Result};
use crate::resources::Resources;
use crate::runtime::Runtime;
use crate::session::{containerfile, Reporter};

/// Pulls (or builds) the images and optionally archives them.
pub fn prepare(args: &PrepareArgs, rep: &Reporter) -> Result<()> {
    let rt = Runtime::detect(args.runtime)?;
    rep.debug(format!("using container runtime '{}'", rt.bin));

    if args.build {
        let resources = Resources::new(rt.clone());
        let context = resources.claim_temp_dir();
        std::fs::create_dir_all(&context)?;
        rep.info(format!("Building {} ...", args.sadf_image));
        if !rt.build(&args.sadf_image, containerfile(), &context) {
            return Err(Error::other(format!(
                "building {} failed (the build needs network access)",
                args.sadf_image
            )));
        }
    } else {
        pull(&rt, &args.sadf_image, rep)?;
    }
    pull(&rt, &args.grafana_image, rep)?;

    if let Some(archive) = &args.save {
        rep.info(format!("Saving both images to {} ...", archive.display()));
        if !rt.save(archive, &[&args.sadf_image, &args.grafana_image]) {
            return Err(Error::other(format!(
                "saving the images to {} failed",
                archive.display()
            )));
        }
        rep.info(format!(
            "Done. On the air-gapped host: sarda load {}",
            archive.display()
        ));
    } else {
        rep.info("Done. Both images are available locally.");
    }
    Ok(())
}

/// A `localhost/` image can only be built or loaded, so it is never pulled.
fn pull(rt: &Runtime, image: &str, rep: &Reporter) -> Result<()> {
    if image.starts_with("localhost/") {
        if rt.image_exists(image) {
            return Ok(());
        }
        return Err(Error::new(
            Kind::MissingImage,
            format!("{image} is a local image and cannot be pulled; build it with `sarda prepare --build --sadf-image {image}`"),
        ));
    }
    rep.info(format!("Pulling {image} ..."));
    if rt.pull(image) {
        Ok(())
    } else {
        Err(Error::new(
            Kind::MissingImage,
            format!("could not pull {image}; is this host connected to the registry?"),
        ))
    }
}

/// Loads an archive into the container storage.
pub fn load(args: &LoadArgs, rep: &Reporter) -> Result<()> {
    if !args.file.is_file() {
        return Err(Error::new(
            Kind::Usage,
            format!("{} is not a file", args.file.display()),
        ));
    }
    let rt = Runtime::detect(args.runtime)?;
    rep.info(format!("Loading {} ...", args.file.display()));
    if rt.load(&args.file) {
        rep.info("Done. sarda can now run offline.");
        Ok(())
    } else {
        Err(Error::other(format!(
            "loading {} failed",
            args.file.display()
        )))
    }
}
