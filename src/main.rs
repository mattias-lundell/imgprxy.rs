#![feature(plugin, custom_derive)]
#![plugin(rocket_codegen)]

extern crate rocket;
extern crate reqwest;
extern crate image;
extern crate url;

#[macro_use]
extern crate error_chain;
use std::io::copy;
use std::io::{Cursor};
use image::GenericImage;
use rocket::request::FromFormValue;
use rocket::http::RawStr;
use url::{Url};

error_chain! {
    foreign_links {
        ReqError(reqwest::Error);
        IoError(std::io::Error);
        ImageError(image::ImageError);
    }
}

struct Image {
    data: image::DynamicImage
}

impl Image {
    fn as_cursor(self) -> Cursor<Vec<u8>> {
        let mut c = Cursor::new(Vec::new());
        self.data.save(&mut c, image::JPEG).unwrap();
        c
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

// scale image so that the whole image fits inside rectangle given by height and width.
fn fit(img: image::DynamicImage, height: Option<u32>, width: Option<u32>) -> Image {
    let old_h = height.unwrap_or(img.height());
    let old_w = width.unwrap_or(img.width());

    let thumbnail = img.resize(old_w, old_h, image::FilterType::Lanczos3);

    Image{data: thumbnail}
}

// scale image so that it fills up whole rectangle given by height and width,
// then crop image to rectangle.
fn fill(img: image::DynamicImage, height: Option<u32>, width: Option<u32>) -> Image {
    let old_h = img.height() as f32;
    let old_w = img.width() as f32;

    let target_h = height.unwrap_or(img.height()) as f32;
    let target_w = width.unwrap_or(img.width()) as f32;

    let ratio_h = target_h / old_h;
    let ratio_w = target_w / old_w;
    let ratio = ratio_w.max(ratio_h);

    let new_h = old_h * ratio;
    let new_w = old_w * ratio;

    let y_0 = (((new_h - target_h) as f32) / 2.0) as u32;
    let x_0 = (((new_w - target_w) as f32) / 2.0) as u32;

    let mut thumbnail = img.resize(new_w as u32, new_h as u32, image::FilterType::Lanczos3);
    let cropped_thumbnail = thumbnail.crop(x_0, y_0, target_w as u32, target_h as u32);

    Image{data: cropped_thumbnail}
}

fn resize_image(img: image::DynamicImage, mode: String, height: Option<u32>, width: Option<u32>) -> Result<Image> {
    if mode == "fill" {
        return Ok(fill(img, height, width));
    } else if mode == "fit" {
        return Ok(fit(img, height, width));
    }
    unreachable!();
}

fn download_image(url: &str) -> Result<image::DynamicImage> {
    let mut res = reqwest::get(url)?;
    if res.status().is_success() {
        let len = res
            .headers()
            .get::<reqwest::header::ContentLength>()
            .map(|ct_len| **ct_len)
            .unwrap_or(0);
        if len <= 0 {
            Err("ContentLength to small".into())
        } else {
            let mut buf = Vec::with_capacity(len as usize);
            copy(&mut res, &mut buf)?;
            let img = image::load_from_memory(buf.as_slice())?;
            Ok(img)
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
    width: Option<u32>
}

#[derive(Debug)]
struct ValidUrl(Url);

impl<'v> FromFormValue<'v> for ValidUrl {
    type Error = Error;

    fn from_form_value(form_value: &'v RawStr) -> Result<ValidUrl> {
        match form_value.parse::<Url>() {
            Ok(url) => {
                Ok(ValidUrl(url))
            },
            _ => Err("Invalid URL".into()),
        }
    }
}

#[get("/resize?<request>")]
fn retrieve(request: ResizeRequest) -> Result<Image> {
    let img = download_image(request.url.0.as_str())?;
    resize_image(
        img,
        request.mode.unwrap_or("fit".to_string()),
        request.height,
        request.width
    )
}

fn main() {
    rocket::ignite().mount("/", routes![retrieve]).launch();
}
