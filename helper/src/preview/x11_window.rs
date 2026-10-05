use x11rb::connection::Connection;
use x11rb::image::{Image, PixelLayout};
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::{
    ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask, Screen, WindowClass,
};
use x11rb::protocol::Event;

use super::{bottom_right_origin, pick_monitor, Monitor, Rgb, BACKGROUND};
use crate::x11::Display;
use crate::Result;

fn primary_monitor(display: &Display) -> Monitor {
    let screen = display.screen();
    let whole_screen = Monitor {
        x: 0,
        y: 0,
        width: i32::from(screen.width_in_pixels),
        height: i32::from(screen.height_in_pixels),
    };
    let Some(reply) = display
        .conn
        .randr_get_monitors(screen.root, true)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
    else {
        return whole_screen;
    };
    let monitors: Vec<(Monitor, bool)> = reply
        .monitors
        .iter()
        .map(|info| {
            let monitor = Monitor {
                x: i32::from(info.x),
                y: i32::from(info.y),
                width: i32::from(info.width),
                height: i32::from(info.height),
            };
            (monitor, info.primary)
        })
        .collect();
    pick_monitor(&monitors).copied().unwrap_or(whole_screen)
}

fn native_image(display: &Display, screen: &Screen, picture: &Rgb) -> Result<Image<'static>> {
    let visual = screen
        .allowed_depths
        .iter()
        .flat_map(|depth| &depth.visuals)
        .find(|visual| visual.visual_id == screen.root_visual)
        .ok_or("the screen's visual is missing")?;
    let layout = PixelLayout::from_visual_type(*visual)?;
    let mut image = Image::allocate_native(
        picture.width as u16,
        picture.height as u16,
        screen.root_depth,
        display.conn.setup(),
    )?;
    for (index, pixel) in picture.pixels.chunks_exact(3).enumerate() {
        let widen = |channel: u8| u16::from(channel) * 257;
        let encoded = layout.encode((widen(pixel[0]), widen(pixel[1]), widen(pixel[2])));
        image.put_pixel(
            (index as u32 % picture.width) as u16,
            (index as u32 / picture.width) as u16,
            encoded,
        );
    }
    Ok(image)
}

pub(super) fn show(compose: impl FnOnce(&Monitor) -> Result<Rgb>) -> Result<()> {
    let display = Display::connect()?;
    let screen = display.screen();
    let monitor = primary_monitor(&display);
    let picture = compose(&monitor)?;
    let (x, y) = bottom_right_origin(&monitor, picture.width, picture.height);
    let image = native_image(&display, screen, &picture)?;

    let window = display.conn.generate_id()?;
    let gc = display.conn.generate_id()?;
    let background =
        u32::from(BACKGROUND[0]) << 16 | u32::from(BACKGROUND[1]) << 8 | u32::from(BACKGROUND[2]);
    display.conn.create_window(
        screen.root_depth,
        window,
        screen.root,
        x as i16,
        y as i16,
        picture.width as u16,
        picture.height as u16,
        0,
        WindowClass::INPUT_OUTPUT,
        screen.root_visual,
        &CreateWindowAux::new()
            .override_redirect(1)
            .background_pixel(background)
            .event_mask(EventMask::EXPOSURE | EventMask::BUTTON_PRESS),
    )?;
    display.conn.create_gc(gc, window, &CreateGCAux::new())?;
    display.conn.map_window(window)?;
    display.conn.flush()?;

    loop {
        match display.conn.wait_for_event()? {
            Event::Expose(event) if event.count == 0 => {
                image.put(&display.conn, window, gc, 0, 0)?;
                display.conn.flush()?;
            }
            Event::ButtonPress(_) => return Ok(()),
            _ => {}
        }
    }
}
