use super::vat_types::{
    VatCapData, VatExportID, VatMessage, VatPromiseID, VatResolverID, VatSendTarget,
};

// TODO: we need a name for the pass-by-presence type. "target"? "export"?

pub trait Dispatch {
    fn deliver(
        &mut self,
        target: VatExportID,
        message: VatMessage,
        resolver: Option<VatResolverID>,
    );
    fn notify_fulfill_to_target(&mut self, id: VatPromiseID, target: VatSendTarget);
    fn notify_fulfill_to_data(&mut self, id: VatPromiseID, data: VatCapData);
    fn notify_reject(&mut self, id: VatPromiseID, data: VatCapData);
}
