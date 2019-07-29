use super::kernel::{
    CapData as KernelCapData, CapSlot as KernelCapSlot, Message as KernelMessage,
    ObjectID as KernelObjectID, ObjectTable as KernelObjectTable, PendingDelivery,
    PromiseID as KernelPromiseID, PromiseTable as KernelPromiseTable,
    Resolution as KernelResolution, VatID,
};
use super::vat::{
    CapData as VatCapData, CapSlot as VatCapSlot, InboundTarget, Message as VatMessage,
    ObjectID as VatObjectID, PromiseID as VatPromiseID, Resolution as VatResolution,
    Syscall,
};
use super::vat_data::VatData as KernelVatData;

fn map_outbound_promise(
    vd: &mut KernelVatData,
    pt: &mut KernelPromiseTable,
    id: VatPromiseID,
) -> KernelPromiseID {
    // this is not for answers
    let decider = vd.id;
    let allocator = vd.id;
    let allocate = || pt.allocate_unresolved(decider, allocator);
    vd.promise_clist.map_outbound(id, allocate)
}

fn map_outbound_slot(
    vd: &mut KernelVatData,
    pt: &mut KernelPromiseTable,
    ot: &mut KernelObjectTable,
    slot: VatCapSlot,
) -> KernelCapSlot {
    use VatCapSlot::*;
    match slot {
        Promise(id) => KernelCapSlot::Promise(map_outbound_promise(vd, pt, id)),
        Object(id) => KernelCapSlot::Object({
            let owner = vd.id;
            let allocate = || ot.allocate(owner);
            vd.object_clist.map_outbound(id, allocate)
        }),
    }
}

fn get_outbound_slot(
    vd: &mut KernelVatData,
    pt: &mut KernelPromiseTable,
    ot: &mut KernelObjectTable,
    slot: VatCapSlot,
) -> KernelCapSlot {
    // must already exist
    use VatCapSlot::*;
    match slot {
        Promise(id) => KernelCapSlot::Promise(vd.promise_clist.get_outbound(id).unwrap()),
        Object(id) => KernelCapSlot::Object(vd.object_clist.get_outbound(id).unwrap()),
    }
}

fn map_outbound_capdata(
    vd: &mut KernelVatData,
    pt: &mut KernelPromiseTable,
    ot: &mut KernelObjectTable,
    data: VatCapData,
) -> KernelCapData {
    KernelCapData {
        body: data.body,
        slots: data
            .slots
            .iter()
            .map(|s| map_outbound_slot(vd, pt, ot, *s))
            .collect(),
    }
}

fn map_outbound_result(
    vd: &mut KernelVatData,
    pt: &mut KernelPromiseTable,
    target_vatid: VatID,
    id: VatPromiseID,
) -> KernelPromiseID {
    // this is only for answers
    let decider = target_vatid;
    let allocator = vd.id;
    let allocate = || pt.allocate_unresolved(decider, allocator);
    vd.promise_clist.map_outbound(id, allocate)
}

fn map_outbound_message(
    vd: &mut KernelVatData,
    pt: &mut KernelPromiseTable,
    ot: &mut KernelObjectTable,
    target_vatid: VatID,
    message: VatMessage,
) -> KernelMessage {
    KernelMessage {
        method: message.method,
        args: map_outbound_capdata(vd, pt, ot, message.args),
        result: message
            .result
            .map(|rp| map_outbound_result(vd, pt, target_vatid, rp)),
    }
}

fn map_outbound_send(
    vd: &mut KernelVatData,
    pt: &mut KernelPromiseTable,
    ot: &mut KernelObjectTable,
    target: VatCapSlot,
    message: VatMessage,
) -> PendingDelivery {
    // look up the target first, promise or object, and find it's decider/owner
    // then if a result promise must be allocated, use that as the decider
    let kt = get_outbound_slot(vd, pt, ot, target); // must already exist
    use KernelCapSlot::*;
    let target_vatid = match kt {
        Promise(id) => pt.decider_of(id),
        Object(id) => ot.owner_of(id),
    };
    let km = KernelMessage {
        method: message.method,
        args: map_outbound_capdata(vd, pt, ot, message.args),
        result: message
            .result
            .map(|rp| map_outbound_result(vd, pt, target_vatid, rp)),
    };
    PendingDelivery::Deliver {
        target: kt,
        message: km,
    }
}

fn get_outbound_promise(
    vd: &mut KernelVatData,
    pt: &mut KernelPromiseTable,
    id: VatPromiseID,
) -> KernelPromiseID {
    // this is for resolutions, not for answers. must already exist
    // TODO: check that the sending vat is the decider
    vd.promise_clist.get_outbound(id).unwrap()
}

fn map_outbound_resolution(
    vd: &mut KernelVatData,
    pt: &mut KernelPromiseTable,
    ot: &mut KernelObjectTable,
    resolution: VatResolution,
) -> KernelResolution {
    use VatResolution::*;
    match resolution {
        Reference(vslot) => {
            KernelResolution::Reference(map_outbound_slot(vd, pt, ot, vslot))
        }
        Data(vdata) => KernelResolution::Data(map_outbound_capdata(vd, pt, ot, vdata)),
        Rejection(vdata) => {
            KernelResolution::Rejection(map_outbound_capdata(vd, pt, ot, vdata))
        }
    }
}