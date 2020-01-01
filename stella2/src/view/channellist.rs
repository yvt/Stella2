use std::ops::Range;
use tcw3::{
    pal,
    ui::{
        prelude::*,
        theming,
        views::{table, table::LineTy, Label, ScrollableTable},
    },
    uicore::{HView, SizeTraits},
};

pub struct ChannelListView {
    table: ScrollableTable,
}

impl ChannelListView {
    pub fn new(_: pal::Wm, style_manager: &'static theming::Manager) -> Self {
        let table = ScrollableTable::new(style_manager);

        // This minimum size is kind of arbitrary
        table.table().set_size_traits(SizeTraits {
            preferred: [150.0, 200.0].into(),
            min: [40.0, 40.0].into(),
            ..Default::default()
        });

        // Set up the table model
        {
            let mut edit = table.table().edit().unwrap();
            edit.set_model(TableModelQuery { style_manager });
            edit.insert(LineTy::Row, 0..30);
            edit.insert(LineTy::Col, 0..1);
            edit.set_scroll_pos([0.0, edit.scroll_pos()[1]]);
        }

        Self { table }
    }

    pub fn view(&self) -> &HView {
        self.table.view()
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
