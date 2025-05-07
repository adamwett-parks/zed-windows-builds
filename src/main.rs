use std::{env, fs};
use std::{error::Error, io::Write as _};
use std::{io::Cursor, str::FromStr as _};
use std::{path::Path, process};

use comrak::{
    format_commonmark, nodes::NodeValue, parse_document, Arena, ExtensionOptions, ListStyleType,
    RenderOptions,
};
use crossterm::{event::read, event::Event as CEvent};
use strum::{EnumProperty, EnumString, VariantNames};
use zip::read::ZipArchive;

#[derive(EnumString, VariantNames, EnumProperty)]
#[strum(ascii_case_insensitive)]
enum Asset {
    #[strum(props(name = "zed-opengl.exe"))]
    OpenGl,
    #[strum(props(name = "zed-opengl.zip"))]
    ZipOpenGl,
    #[strum(props(name = "zed.exe"))]
    Vulkan,
    #[strum(props(name = "zed.zip"))]
    ZipVulkan,
}

fn help() -> ! {
    let mut msg = "zed-dl <ASSET>\n\nAsset types are as follows (case-insensitive):".to_owned();

    for name in Asset::VARIANTS {
        msg.push('\n');
        msg.push_str(name);
    }

    println!("{msg}");
    process::exit(1);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let Some(asset) = env::args().nth(1) else {
        help();
    };

    match &*asset {
        "--help" | "-h" => {
            help();
        }

        _ => (),
    }

    let looking_for_asset = match Asset::from_str(&asset) {
        Ok(a) => a.get_str("name").unwrap(),
        Err(_) => help(),
    };

    let octocrab = octocrab::instance();

    let latest = octocrab
        .repos("MolotovCherry", "zed-windows-builds")
        .releases()
        .get_latest()
        .await?;

    let tag = latest.tag_name;

    println!("Found release {tag}");

    let asset = latest
        .assets
        .iter()
        .find(|asset| asset.name.eq_ignore_ascii_case(looking_for_asset))
        .ok_or_else(|| format!("No asset found on latest release {tag}"))?;

    println!("Downloading asset {}", asset.name);

    let data = reqwest::get(asset.browser_download_url.clone())
        .await?
        .bytes()
        .await?;

    let path = Path::new(&asset.name);

    let ext = path
        .extension()
        .ok_or("asset has no extension")?
        .to_string_lossy();

    match &*ext.to_ascii_lowercase() {
        "zip" => {
            let cursor = Cursor::new(&data);
            let mut zip = ZipArchive::new(cursor)?;

            zip.extract(".")?;

            for filename in zip.file_names() {
                println!("File: {filename}");
            }
        }

        "exe" => {
            fs::write(&asset.name, data)?;
            println!("File: {}", asset.name);
        }

        _ => Err(format!("extension {ext} is unsupported"))?,
    }

    if let Some(body) = latest.body {
        let arena = Arena::new();

        let options = comrak::Options {
            extension: ExtensionOptions {
                autolink: false,
                ..Default::default()
            },
            parse: Default::default(),
            render: RenderOptions {
                list_style: ListStyleType::Star,
                prefer_fenced: true,
                ignore_empty_links: true,
                ..Default::default()
            },
        };

        let root = parse_document(&arena, &body, &options);

        let mut detaches = Vec::new();

        for node in root.descendants() {
            let node_val = &mut node.data.borrow_mut().value;

            #[allow(clippy::single_match)]
            match node_val {
                // remove links
                NodeValue::Link(..) => {
                    let text = node
                        .children()
                        .next()
                        .map(|c| match &c.data.borrow().value {
                            NodeValue::Text(t) => {
                                detaches.push(c);
                                t.clone()
                            }
                            _ => String::new(),
                        })
                        .unwrap_or(String::new());

                    *node_val = NodeValue::Text(text);
                }

                _ => (),
            }
        }

        for node in detaches {
            node.detach();
        }

        let mut output = Vec::new();
        format_commonmark(root, &options, &mut output).expect("failed to format");
        let data = String::from_utf8(output).unwrap();

        println!("\n{}", termimad::term_text(&data));
        pause();
    }

    Ok(())
}

pub fn pause() {
    print!("Press any key to continue...");
    std::io::stdout().flush().unwrap();

    loop {
        match read().unwrap() {
            CEvent::Key(_event) => break,
            _ => continue,
        }
    }
}
