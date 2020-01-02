use std::ops::Range;
use tcw3::{
    ui::{
        mixins::scrollwheel::ScrollAxisFlags,
        prelude::*,
        theming,
        views::{table, table::LineTy, Label},
    },
    uicore::{HView, SizeTraits},
};

stella2_meta::designer_impl! {
    crate::view::channellist::ChannelListView
}

impl ChannelListView {
    fn init(&self) {
        // Set up the table model
        {
            let mut edit = self.table().table().edit().unwrap();
            edit.set_model(TableModelQuery {
                style_manager: self.style_manager(),
            });
            edit.insert(LineTy::Row, 0..30);
            edit.insert(LineTy::Col, 0..1);
            edit.set_scroll_pos([0.0, edit.scroll_pos()[1]]);
        }
    }
}

struct TableModelQuery {
    style_manager: &'static theming::Manager,
}

impl table::TableModelQuery for TableModelQuery {
    fn new_view(&mut self, cell: table::CellIdx) -> (HView, Box<dyn table::CellCtrler>) {
        let label = Label::new(self.style_manager);
        label.set_text(format!("Item {}", cell[1]));

        (label.view().clone(), Box::new(()))
    }

    fn range_size(&mut self, line_ty: LineTy, range: Range<u64>, _approx: bool) -> f64 {
        (range.end - range.start) as f64
            * match line_ty {
                LineTy::Row => 20.0,
                LineTy::Col => 50.0, // TODO: find a better way to fill the width
            }
    }
}
