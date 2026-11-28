#![no_std]
use liuli::{
    component::{Component, container, leaf},
    prelude::*,
};

pub const BRAND: Rgb565 = Rgb565::new(31, 0, 31);
static PAINTER: PartPainter = PartPainter(|part, _, canvas| {
    if part.role == WidgetPart::Icon {
        canvas.fill(part.bounds, BRAND)?;
        return Ok(PaintOverride::Handled);
    }
    Ok(PaintOverride::Default)
});

pub fn record(id: u32) -> impl Component {
    let key = 100 + id * 3;
    container(
        Key(key),
        NodeKind::Row,
        || NodeProps::new().height(Length::Px(24)),
        (
            leaf(Key(key + 1), NodeKind::Label, || {
                NodeProps::new().text(TextRef::Static("A"))
            }),
            leaf(Key(key + 2), NodeKind::Button, || {
                NodeProps::new()
                    .width(Length::Px(40))
                    .height(Length::Px(24))
                    .icon(IconKind::Custom)
                    .with_parts(WidgetPartStyle::ZERO.with_painter(Some(&PAINTER)))
            }),
        ),
    )
}
