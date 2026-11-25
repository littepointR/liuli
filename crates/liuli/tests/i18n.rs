use liuli::prelude::*;

const KEY_GREETING: TextKey = TextKey(0);
const KEY_COUNT: TextKey = TextKey(1);
const KEY_ONLY_EN: TextKey = TextKey(2);

static LOCALES: &[LocaleTable] = &[
    LocaleTable::new("en", &["Hello {0}", "{0} items", "Fallback only"]),
    LocaleTable::new("zh", &["你好 {0}", "共 {0} 项", ""]),
];

struct I18nModel {
    locale: LocaleId,
    name: &'static str,
    count: u64,
}

struct I18nTestApp;

impl App for I18nTestApp {
    type Model = I18nModel;
    type Msg = LocaleId;

    fn on_action(_: &Self::Model, _: UiAction) -> Option<Self::Msg> {
        None
    }

    fn update(model: &mut Self::Model, msg: LocaleId, _now: InstantMs) -> Change {
        model.locale = msg;
        Change::View
    }

    fn view(model: &Self::Model, vb: &mut ViewBuilder<'_>) -> Result<(), BuildError> {
        let greet_tmpl = LocaleTable::resolve(LOCALES, model.locale, KEY_GREETING);
        let fallback = LocaleTable::resolve(LOCALES, model.locale, KEY_ONLY_EN);
        let plural_suffix = match plural_en(model.count) {
            PluralForm::One => "item",
            PluralForm::Other => "items",
        };
        vb.root(Key(1), NodeProps::new().bg(Rgb565::new(0, 0, 0)), |ui| {
            let txt = ui.text_fmt(format_args!(
                "{} ({})",
                TemplateDisplay::new(greet_tmpl, &[&model.name]),
                plural_suffix
            ))?;
            ui.label(
                Key(2),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Px(24))
                    .text(txt),
            )?;
            ui.label(
                Key(3),
                NodeProps::new()
                    .width(Length::Fill(1))
                    .height(Length::Px(24))
                    .text(TextRef::Static(fallback)),
            )
        })
    }
}

#[test]
fn locale_resolution_fallback_plurals_and_template_rendering() {
    assert_eq!(plural_en(0), PluralForm::Other);
    assert_eq!(plural_en(1), PluralForm::One);
    assert_eq!(plural_en(2), PluralForm::Other);
    assert_eq!(plural_cjk(1), PluralForm::Other);

    let zh = LocaleTable::find_locale(LOCALES, "ZH").unwrap();
    assert_eq!(zh, LocaleId(1));
    assert_eq!(
        LocaleTable::resolve(LOCALES, zh, KEY_ONLY_EN),
        "Fallback only"
    );
    assert_eq!(
        LocaleTable::resolve(LOCALES, LocaleId(99), KEY_COUNT),
        "{0} items"
    );
    assert_eq!(LocaleTable::resolve(LOCALES, zh, TextKey(42)), "");

    let lf4 = include_bytes!("../../../apps/liuli-alarms/assets/body.lf4");
    let fonts = FontSet::single(Lf4Font::from_bytes(lf4).unwrap());
    let mut storage = Storage::new();
    let model = I18nModel {
        locale: LocaleId(0),
        name: "Liuli",
        count: 3,
    };
    let mut engine = Engine::<I18nTestApp>::new_in_place_with_fonts(
        &mut storage,
        model,
        fonts,
        Rect::new(0, 0, 240, 120),
        InstantMs(0),
    )
    .unwrap();

    let tree0 = engine.active_tree();
    let s2 = tree0.find_slot_by_key(Key(2)).unwrap() as usize;
    assert_eq!(
        tree0.text_str(tree0.nodes[s2].props.text()),
        "Hello Liuli (items)"
    );

    engine.send(zh, InstantMs(10)).unwrap();
    let tree1 = engine.active_tree();
    let s2 = tree1.find_slot_by_key(Key(2)).unwrap() as usize;
    let s3 = tree1.find_slot_by_key(Key(3)).unwrap() as usize;
    assert_eq!(
        tree1.text_str(tree1.nodes[s2].props.text()),
        "你好 Liuli (items)"
    );
    assert_eq!(
        tree1.text_str(tree1.nodes[s3].props.text()),
        "Fallback only"
    );
}
