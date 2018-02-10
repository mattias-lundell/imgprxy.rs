#![feature(plugin, custom_derive)]
#![plugin(rocket_codegen)]

extern crate envy;
extern crate image;
extern crate reqwest;
extern crate rocket;
extern crate url;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate error_chain;

use std::collections::HashSet;
use std::io::copy;
use std::io::Cursor;
use image::GenericImage;
use rocket::request::FromFormValue;
use rocket::http::RawStr;
use url::Url;

error_chain! {
    foreign_links {
        ReqError(reqwest::Error);
        IoError(std::io::Error);
        ImageError(image::ImageError);
    }
}

struct Image {
    data: image::DynamicImage,
}

impl Image {
    fn as_cursor(self) -> Cursor<Vec<u8>> {
        let mut c = Cursor::new(Vec::new());
        self.data.save(&mut c, image::JPEG).unwrap();
        c
    }

    // scale image so that the whole image fits inside rectangle given by height and width.
    fn fit(self, height: Option<u32>, width: Option<u32>) -> Image {
        let old_h = height.unwrap_or(self.data.height());
        let old_w = width.unwrap_or(self.data.width());

        let thumbnail = self.data.resize(old_w, old_h, image::FilterType::Lanczos3);

        Image { data: thumbnail }
    }

    // scale image so that it fills up whole rectangle given by height and width,
    // then crop image to rectangle.
    fn fill(self, height: Option<u32>, width: Option<u32>) -> Image {
        let old_h = self.data.height() as f32;
        let old_w = self.data.width() as f32;

        let target_h = height.unwrap_or(self.data.height()) as f32;
        let target_w = width.unwrap_or(self.data.width()) as f32;

        let ratio_h = target_h / old_h;
        let ratio_w = target_w / old_w;
        let ratio = ratio_w.max(ratio_h);

        let new_h = old_h * ratio;
        let new_w = old_w * ratio;

        let y_0 = (((new_h - target_h) as f32) / 2.0) as u32;
        let x_0 = (((new_w - target_w) as f32) / 2.0) as u32;

        let mut thumbnail =
            self.data
                .resize(new_w as u32, new_h as u32, image::FilterType::Lanczos3);
        let cropped_thumbnail = thumbnail.crop(x_0, y_0, target_w as u32, target_h as u32);

        Image {
            data: cropped_thumbnail,
        }
    }
}

impl<'r> rocket::response::Responder<'r> for Image {
    fn respond_to(self, _: &rocket::Request) -> rocket::response::Result<'r> {
        rocket::Response::build()
            .header(rocket::http::ContentType::JPEG)
            .sized_body(self.as_cursor())
            .ok()
    }
}

fn resize_image(
    img: Image,
    mode: String,
    height: Option<u32>,
    width: Option<u32>,
) -> Result<Image> {
    if mode == "fill" {
        return Ok(img.fill(height, width));
    } else if mode == "fit" {
        return Ok(img.fit(height, width));
    }
    unreachable!();
}

fn download_image(url: &str) -> Result<Image> {
    let mut res = reqwest::get(url)?;
    if res.status().is_success() {
        let len = res.headers()
            .get::<reqwest::header::ContentLength>()
            .map(|ct_len| **ct_len)
            .unwrap_or(0);
        if len <= 0 {
            Err("ContentLength to small".into())
        } else {
            let mut buf = Vec::with_capacity(len as usize);
            copy(&mut res, &mut buf)?;
            let img = image::load_from_memory(buf.as_slice())?;
            Ok(Image { data: img })
        }
    } else {
        Err("Request was not successful".into())
    }
}

#[derive(Debug, FromForm)]
struct ResizeRequest {
    url: ValidUrl,
    mode: Option<String>,
    height: Option<u32>,
    width: Option<u32>,
}

#[derive(Debug)]
struct ValidUrl(Url);

impl<'v> FromFormValue<'v> for ValidUrl {
    type Error = Error;

    fn from_form_value(form_value: &'v RawStr) -> Result<ValidUrl> {
        match form_value.parse::<Url>() {
            Ok(url) => Ok(ValidUrl(url)),
            _ => Err("Invalid URL".into()),
        }
    }
}

#[derive(Deserialize, Debug)]
struct Config {
    url_whitelist: Vec<String>,
}

lazy_static! {
    static ref URL_WHITELIST: HashSet<String> = {
        let mut m = HashSet::new();
        for url in envy::from_env::<Config>().unwrap().url_whitelist {
            m.insert(url);
        }
        m
    };
}

fn valid_host(url: &Url) -> bool {
    match url.host_str() {
        Some(url) => URL_WHITELIST.contains(url),
        None => false,
    }
}

#[get("/resize?<request>")]
fn retrieve(request: ResizeRequest) -> Result<Image> {
    if valid_host(&request.url.0) {
        let img = download_image(request.url.0.as_str())?;
        return resize_image(
            img,
            request.mode.unwrap_or("fit".to_string()),
            request.height,
            request.width,
        );
    }
    Err("Invalid hostname".into())
}

fn main() {
    rocket::ignite().mount("/", routes![retrieve]).launch();
}
