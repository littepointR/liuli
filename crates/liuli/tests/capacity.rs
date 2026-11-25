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

    #[cfg(not(any(feature = "capacity-128", feature = "capacity-256")))]
    assert!(
        total_core <= 128 * 1024,
        "Storage + 2 DMA bands ({total_core} B) exceeded 128 KiB!"
    );
}

#[test]
fn capacity_profile_scales_max_nodes_beyond_64() {
    let font = Lf4Font::from_bytes(SETTINGS_LF4).unwrap();
    let screen = Rect::new(0, 0, 320, 240);
    let mut t1 = Box::new(RetainedTree::new());
    let mut t2 = Box::new(RetainedTree::new());
    let mut draft = Box::new(ViewDraft::new());
    let mut anims = AnimPool::new();
    let mut dmg = DamageSet::new();

    {
        let mut vb = ViewBuilder::new(&mut draft);
        let target_children = MAX_NODES - 4;
        vb.root(Key(1), NodeProps::new(), |ui| {
            for i in 0..target_children {
                ui.label(Key(100 + i as u32), NodeProps::new())?;
            }
            Ok(())
        })
        .unwrap();
        vb.finish().unwrap();
    }

    reconcile_into(
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
    )
    .unwrap();
    assert_eq!(t2.active_count as usize, MAX_NODES - 3);
    let _ = &mut t1;
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

    // Prime slot 1's generation high-water mark to u32::MAX; reconciler must skip slot 1 and use slot 2.
    t1.generation_hwm[1] = u32::MAX;

    {
        let mut vb = ViewBuilder::new(&mut draft);
        vb.root(Key(1), NodeProps::new(), |ui| {
            ui.label(Key(2), NodeProps::new())?;
            Ok(())
        })
        .unwrap();
        vb.finish().unwrap();
    }

    let ok = reconcile_into(
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
    assert!(ok.is_ok());
    assert_eq!(t2.find_slot_by_key(Key(2)), Some(2));
    assert_eq!(t2.generation_hwm[1], u32::MAX);

    // Exhaust every free slot -> must fail cleanly with BuildError::GenerationExhausted.
    t1.generation_hwm = [u32::MAX; liuli::core::capacity::MAX_NODES];
    dmg.clear();
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
