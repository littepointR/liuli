use core::mem::size_of;

use liuli::core::node::{RetainedNode, RetainedTree};
use liuli::core::paint::{CommandList, PaintCmd};
use liuli::core::reconcile::reconcile_into;
use liuli::core::time::AnimPool;
use liuli::core::view::{ViewBuilder, ViewDraft};
use liuli::prelude::*;

const SETTINGS_LF4: &[u8] = include_bytes!("../../../apps/liuli-settings/assets/settings.lf4");

#[test]
fn static_ram_sizes_fit_128_kib_budget() {
    let storage_sz = size_of::<Storage>();
    let dma_bands_sz = 2 * DEFAULT_BAND_PIXELS * size_of::<u16>();
    let total_core = storage_sz + dma_bands_sz;

    println!(
        "size_of(Storage)={} B, size_of(RetainedTree)={} B, size_of(RetainedNode)={} B, size_of(ViewDraft)={} B, size_of(CommandList)={} B, size_of(PaintCmd)={} B, 2xDMA={} B, total={} B",
        storage_sz,
        size_of::<RetainedTree>(),
        size_of::<RetainedNode>(),
        size_of::<ViewDraft>(),
        size_of::<CommandList>(),
        size_of::<PaintCmd>(),
        dma_bands_sz,
        total_core
    );

    assert!(
        total_core <= 128 * 1024,
        "Storage + 2 DMA bands ({total_core} B) exceeded 128 KiB!"
    );
}

#[test]
fn command_list_overflow_returns_err() {
    let mut cmds = CommandList::new();
    for _ in 0..MAX_COMMANDS {
        cmds.push(PaintCmd::EMPTY).unwrap();
    }
    assert_eq!(cmds.push(PaintCmd::EMPTY), Err(BuildError::Commands));
}

#[test]
fn generation_exhausted_returns_err_without_wraparound() {
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 100, 100);
    let mut t1 = Box::new(RetainedTree::new());
    let mut t2 = Box::new(RetainedTree::new());
    let mut draft = Box::new(ViewDraft::new());
    let mut anims = AnimPool::new();
    let mut dmg = DamageSet::new();

    // Prime slot 1's generation high-water mark to u16::MAX
    t1.generation_hwm[1] = u16::MAX;

    {
        let mut vb = ViewBuilder::new(&mut draft);
        vb.begin(Key(1), NodeKind::Root, NodeProps::new()).unwrap();
        vb.leaf(Key(2), NodeKind::Label, NodeProps::new()).unwrap();
        vb.end().unwrap();
        vb.finish().unwrap();
    }

    let res = reconcile_into(
        &draft,
        &t1,
        &mut t2,
        screen,
        &font,
        &mut anims,
        None,
        None,
        InstantMs(0),
        &mut dmg,
    );
    assert_eq!(res.unwrap_err(), BuildError::GenerationExhausted);
}
