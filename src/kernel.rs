use super::clist::{CList, CListKernelEntry, CListVatEntry};
use super::config::Config;
use super::dispatch::Dispatch;
use super::kernel_types::{
    KernelArgSlot, KernelExport, KernelExportID, KernelMessage, KernelPromiseResolverID,
    VatID, VatName,
};
use super::promise::KernelPromise;
use super::vat::VatSyscall;
use super::vat_types::{
    InboundVatMessage, VatArgSlot, VatCapData, VatExportID, VatImportID, VatPromiseID,
    VatResolverID,
};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

impl CListVatEntry for VatImportID {
    fn new(index: u32) -> Self {
        VatImportID(index)
    }
}
impl CListVatEntry for VatPromiseID {
    fn new(index: u32) -> Self {
        VatPromiseID(index)
    }
}
impl CListVatEntry for VatResolverID {
    fn new(index: u32) -> Self {
        VatResolverID(index)
    }
}
impl CListKernelEntry for KernelExport {}
impl CListKernelEntry for KernelPromiseResolverID {}

#[derive(Debug)]
pub(crate) enum PendingDelivery {
    Deliver {
        target: KernelExport,
        message: KernelMessage,
    },
    DeliverPromise {
        vat: VatID,
        target: KernelPromiseResolverID,
        message: KernelMessage,
    },
    // notify*
}

pub(crate) struct VatData {
    vat_id: VatID,
    pub(crate) import_clist: CList<KernelExport, VatImportID>,
    pub(crate) promise_clist: CList<KernelPromiseResolverID, VatPromiseID>,
    pub(crate) resolver_clist: CList<KernelPromiseResolverID, VatResolverID>,
}
impl VatData {
    pub fn map_inbound_arg_slot(&mut self, slot: KernelArgSlot) -> VatArgSlot {
        match slot {
            KernelArgSlot::Export(ke) => {
                let vat_id: VatID = ke.0;
                if vat_id == self.vat_id {
                    // the vat's own export, returning home
                    let keid: KernelExportID = ke.1;
                    VatArgSlot::Export(VatExportID(keid.0))
                } else {
                    // another vat's export, get/allocate in clist
                    VatArgSlot::Import(self.import_clist.map_inbound(ke))
                }
            }
            KernelArgSlot::Promise(kp) => {
                VatArgSlot::Promise(self.promise_clist.map_inbound(kp))
            }
        }
    }

    pub fn map_inbound_resolver(
        &mut self,
        krid: KernelPromiseResolverID,
    ) -> VatResolverID {
        self.resolver_clist.map_inbound(krid)
    }
    pub fn map_outbound_resolver(
        &mut self,
        vrid: VatResolverID,
    ) -> KernelPromiseResolverID {
        self.resolver_clist.map_outbound(vrid)
    }
}

#[derive(Debug, Default)]
pub(crate) struct RunQueue(pub VecDeque<PendingDelivery>);

pub(crate) struct KernelData {
    pub(crate) vat_names: HashMap<VatName, VatID>,
    pub(crate) vat_data: HashMap<VatID, VatData>,
    pub(crate) run_queue: RunQueue,
    pub(crate) next_promise_resolver_id: u32,
    pub(crate) promises: HashMap<KernelPromiseResolverID, KernelPromise>,
}

//#[derive(Debug)]
pub struct Kernel {
    pub(crate) vat_dispatch: HashMap<VatID, Box<dyn Dispatch>>,
    pub(crate) kd: Rc<RefCell<KernelData>>,
}

impl Kernel {
    pub fn new(cfg: Config) -> Self {
        let mut vat_dispatch = HashMap::new();
        let kd = Rc::new(RefCell::new(KernelData {
            vat_names: HashMap::new(),
            vat_data: HashMap::new(),
            run_queue: RunQueue::default(),
            promises: HashMap::default(),
            next_promise_resolver_id: 0,
        }));
        let mut id = 0;
        for (key, setup) in cfg.vats {
            let vat_id = VatID(id);
            id += 1;
            kd.borrow_mut()
                .vat_names
                .insert(VatName(key.0.clone()), vat_id);
            let vd = VatData {
                vat_id,
                import_clist: CList::new(),
                promise_clist: CList::new(),
                resolver_clist: CList::new(),
            };
            kd.borrow_mut().vat_data.insert(vat_id, vd);
            let syscall = VatSyscall::new(vat_id, kd.clone());
            let dispatch = setup(Box::new(syscall));
            vat_dispatch.insert(vat_id, dispatch);
        }
        Kernel { vat_dispatch, kd }
    }

    /*
    pub fn _add_vat(&mut self, name: &VatName, dispatch: impl Dispatch + 'static) {
        self.vat_dispatch
            .insert(VatName(name.0.clone()), Box::new(dispatch));
        self.vat_data.insert(
            VatName(name.0.clone()),
            VatData {
                import_clist: CList::new(),
                promise_clist: CList::new(),
            },
        );
    }
    */

    pub(crate) fn add_import(
        &mut self,
        for_vat: &VatName,
        for_id: u32,
        to_vat: &VatName,
        to_id: u32,
    ) {
        let mut kd = self.kd.borrow_mut();
        let for_vat_id = *kd.vat_names.get(&for_vat).unwrap();
        let to_vat_id = *kd.vat_names.get(&to_vat).unwrap();
        kd.vat_data.get_mut(&for_vat_id).unwrap().import_clist.add(
            KernelExport(to_vat_id, KernelExportID(to_id)),
            VatImportID(for_id),
        );
    }

    pub(crate) fn push(
        &mut self,
        name: &VatName,
        export: KernelExportID,
        message: KernelMessage,
    ) {
        let mut kd = self.kd.borrow_mut();
        let vat_id = *kd.vat_names.get(&name).unwrap();
        let pd = PendingDelivery::Deliver {
            target: KernelExport(vat_id, export),
            message,
        };
        kd.run_queue.0.push_back(pd);
    }

    /// exports return home with the same index
    fn map_inbound_target(&self, id: KernelExportID) -> VatExportID {
        VatExportID(id.0)
    }

    fn process(&mut self, pd: PendingDelivery) {
        match pd {
            PendingDelivery::Deliver {
                target,
                message: kmsg,
            } => {
                let vat_id = target.0; // TODO nicer destructuring assignment
                let kid = target.1;
                println!("process: {}.{}", target, kmsg.name);
                let veid = self.map_inbound_target(kid);
                let vmsg = {
                    let mut kd = self.kd.borrow_mut();
                    let vd = kd.vat_data.get_mut(&vat_id).unwrap();
                    let ovrid: Option<VatResolverID> = match kmsg.resolver {
                        Some(krid) => Some(vd.map_inbound_resolver(krid)),
                        None => None,
                    };
                    InboundVatMessage {
                        name: kmsg.name,
                        args: VatCapData {
                            body: kmsg.args.body,
                            slots: kmsg
                                .args
                                .slots
                                .into_iter()
                                .map(|slot| vd.map_inbound_arg_slot(slot))
                                .collect(),
                        },
                        resolver: ovrid,
                    }
                };
                let dispatch = self.vat_dispatch.get_mut(&vat_id).unwrap();
                dispatch.deliver(veid, vmsg);
            }
            //PendingDelivery::DeliverPromise(..) => {}
            _ => panic!(),
        };
    }

    pub fn step(&mut self) {
        println!("kernel.step");
        let pdo = self.kd.borrow_mut().run_queue.0.pop_front();
        if let Some(pd) = pdo {
            self.process(pd);
        }
    }

    pub fn run(&mut self) {
        println!("kernel.run");
    }

    pub fn dump(&self) {
        println!("Kernel Dump:");
        println!(" run-queue:");
        for pd in &self.kd.borrow().run_queue.0 {
            println!("  {:?}", pd);
        }
    }
}
