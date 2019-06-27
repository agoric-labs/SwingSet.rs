use super::kernel_types::{
    KernelArgSlot, KernelExport, KernelExportID, KernelPromiseID, KernelResolverID,
    KernelTarget, VatName,
};
use super::vat::{Dispatch, VatSyscall};
use super::vat_types::{VatArgSlot, VatExportID, VatImportID};
use std::collections::{HashMap, VecDeque};

#[derive(Debug)]
pub struct PendingDelivery {
    target: KernelTarget,
    method: String,
    args: u8,
    resolver: KernelResolverID,
}
impl PendingDelivery {
    pub(crate) fn new(
        target: KernelTarget,
        method: &str,
        args: u8,
        resolver: KernelResolverID,
    ) -> Self {
        PendingDelivery {
            target,
            method: method.to_string(),
            args,
            resolver: resolver,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct CList {
    pub inbound: HashMap<KernelArgSlot, VatArgSlot>,
    pub outbound: HashMap<VatArgSlot, KernelArgSlot>,
}
impl CList {
    pub fn _map_outbound<T: Into<VatArgSlot>>(&self, target: T) -> KernelArgSlot {
        let t = self.outbound.get(&target.into()).unwrap();
        (*t).clone()
    }

    pub fn map_outbound_target<T: Into<VatArgSlot>>(&self, target: T) -> KernelTarget {
        let t = self.outbound.get(&target.into()).unwrap();
        match t {
            KernelArgSlot::Export(ke) => KernelTarget::Export(ke.clone()),
            KernelArgSlot::Promise(id) => KernelTarget::Promise(id.clone()),
        }
    }
}

pub struct VatData {
    clist: CList,
    dispatch: Box<dyn Dispatch>,
}

#[derive(Debug, Default)]
pub struct RunQueue(pub VecDeque<PendingDelivery>);

//#[derive(Debug)]
pub struct Kernel {
    vats: HashMap<VatName, VatData>,
    run_queue: RunQueue,
    next_promise_resolver_id: u32,
}

impl Kernel {
    pub fn new(vats: HashMap<VatName, Box<dyn Dispatch>>) -> Self {
        let mut kvats = <HashMap<VatName, VatData>>::new();
        for (key, dispatch) in vats {
            kvats.insert(
                VatName(key.to_string()),
                VatData {
                    clist: CList::default(),
                    dispatch,
                },
            );
        }
        Kernel {
            vats: kvats,
            run_queue: RunQueue::default(),
            next_promise_resolver_id: 0,
        }
    }

    pub fn _add_vat(&mut self, name: &VatName, dispatch: impl Dispatch + 'static) {
        let vn = VatName(name.0.clone());
        self.vats.insert(
            vn,
            VatData {
                clist: CList::default(),
                dispatch: Box::new(dispatch),
            },
        );
    }

    pub(crate) fn add_import(
        &mut self,
        for_vat: &VatName,
        for_id: VatImportID,
        to: KernelExport,
    ) {
        let vslot = VatArgSlot::Import(for_id);
        let kslot = KernelArgSlot::Export(to);
        let clist = &mut self.vats.get_mut(&for_vat).unwrap().clist;
        clist.inbound.insert(kslot.clone(), vslot.clone());
        clist.outbound.insert(vslot, kslot);
    }

    fn allocate_promise_resolver_pair(&mut self) -> (KernelPromiseID, KernelResolverID) {
        let id = self.next_promise_resolver_id;
        self.next_promise_resolver_id += 1;
        (KernelPromiseID(id), KernelResolverID(id))
    }

    pub fn push(&mut self, name: &VatName, export: KernelExportID, method: String) {
        let (_pid, rid) = self.allocate_promise_resolver_pair();
        let pd = PendingDelivery {
            target: KernelTarget::Export(KernelExport(VatName(name.0.clone()), export)),
            method,
            args: 0,
            resolver: rid,
        };
        self.run_queue.0.push_back(pd);
    }

    fn map_export_target(&self, id: KernelExportID) -> VatExportID {
        VatExportID(id.0)
    }

    fn _map_inbound(&mut self, _vn: &VatName, id: KernelExportID) -> VatExportID {
        // todo clist mapping
        VatExportID(id.0)
    }

    fn process(&mut self, pd: PendingDelivery) {
        let t = pd.target;
        println!("process: {}.{}", t, pd.method);
        match t {
            KernelTarget::Export(KernelExport(vn, kid)) => {
                //let vid = self.map_inbound(&vn, kid);
                let vid = self.map_export_target(kid);
                //let VatData{ clist, dispatch } = self.vats.get_mut(&vn).unwrap();
                let vd = self.vats.get_mut(&vn).unwrap();
                let mut syscall = VatSyscall::new(&mut self.run_queue, &mut vd.clist);
                vd.dispatch.deliver(&mut syscall, vid);
            }
            //KernelTarget::Promise(_pid) => {}
            _ => panic!(),
        };
    }

    pub fn step(&mut self) {
        println!("kernel.step");
        if let Some(pd) = self.run_queue.0.pop_front() {
            self.process(pd);
        }
    }

    pub fn run(&mut self) {
        println!("kernel.run");
    }

    pub fn dump(&self) {
        println!("Kernel Dump:");
        println!(" run-queue:");
        for pd in &self.run_queue.0 {
            println!("  {:?}", pd);
        }
    }
}
