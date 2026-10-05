use objc2::{AllocAnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSEventMask, NSEventType,
    NSFloatingWindowLevel, NSImage, NSImageView, NSPanel, NSScreen, NSWindowStyleMask,
};
use objc2_foundation::{NSData, NSDate, NSDefaultRunLoopMode, NSPoint, NSRect, NSSize};

use super::{Monitor, Rgb, SCREEN_MARGIN};
use crate::Result;

fn primary_visible_frame(main_thread: MainThreadMarker) -> Monitor {
    let screens = NSScreen::screens(main_thread);
    let Some(primary) = screens.firstObject() else {
        return Monitor {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        };
    };
    let frame = primary.visibleFrame();
    Monitor {
        x: frame.origin.x as i32,
        y: frame.origin.y as i32,
        width: frame.size.width as i32,
        height: frame.size.height as i32,
    }
}

fn png_bytes(picture: &Rgb) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, picture.width, picture.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.write_header()?.write_image_data(&picture.pixels)?;
    Ok(bytes)
}

fn bottom_right_frame(monitor: &Monitor, picture: &Rgb) -> NSRect {
    let x = monitor.x + monitor.width - picture.width as i32 - SCREEN_MARGIN;
    let y = monitor.y + SCREEN_MARGIN;
    NSRect::new(
        NSPoint::new(f64::from(x), f64::from(y)),
        NSSize::new(f64::from(picture.width), f64::from(picture.height)),
    )
}

pub(super) fn show(compose: impl FnOnce(&Monitor) -> Result<Rgb>) -> Result<()> {
    let main_thread = MainThreadMarker::new().ok_or("the preview must run on the main thread")?;
    let monitor = primary_visible_frame(main_thread);
    let picture = compose(&monitor)?;

    let app = NSApplication::sharedApplication(main_thread);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let data = NSData::with_bytes(&png_bytes(&picture)?);
    let image =
        NSImage::initWithData(NSImage::alloc(), &data).ok_or("could not load the preview image")?;
    let view = NSImageView::imageViewWithImage(&image, main_thread);
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(main_thread),
        bottom_right_frame(&monitor, &picture),
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    panel.setLevel(NSFloatingWindowLevel);
    panel.setContentView(Some(&view));
    panel.orderFrontRegardless();

    loop {
        let event = unsafe {
            app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                Some(&NSDate::distantFuture()),
                NSDefaultRunLoopMode,
                true,
            )
        };
        let Some(event) = event else { continue };
        if event.r#type() == NSEventType::LeftMouseDown {
            return Ok(());
        }
        app.sendEvent(&event);
    }
}
