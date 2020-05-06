use std::{ops::Range, rc::Rc};
use tcw3::{
    ui::{
        prelude::*,
        theming,
        views::{table, table::LineTy, Button, Label},
    },
    uicore::{HView, HViewRef},
};

use crate::stylesheet::{elem_id, my_roles};

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
                elem: Rc::clone(self.elem()),
            });
            edit.insert(LineTy::Row, 0..29);
            edit.insert(LineTy::Col, 0..1);
            edit.set_scroll_pos([0.0, 0.0]);
        }
    }
}

impl theming::Widget for ChannelListView {
    fn view_ref(&self) -> HViewRef<'_> {
        self.view().as_ref()
    }

    fn style_elem(&self) -> Option<theming::HElem> {
        Some(self.style_elem())
    }
}

struct TableModelQuery {
    style_manager: &'static theming::Manager,
    elem: Rc<theming::Elem>,
}

impl table::TableModelQuery for TableModelQuery {
    fn new_view(&mut self, cell: table::CellIdx) -> (HView, Box<dyn table::CellCtrler>) {
        let label = Label::new(self.style_manager);
        label.set_text(match (cell[1] % 4, (cell[1] / 4) % 4) {
            (0, 0) => "randomserver — Slack",
            (0, 1) => "workplace — Slack",
            (0, 2) => "thawedpeach — GNU Social",
            (0, 3) => "FreeNode",
            (1, _) => "#general",
            (2, _) => "#prolang",
            (3, _) => "#random",
            _ => unreachable!(),
        });

        let wrap = theming::StyledBox::new(self.style_manager, Default::default());
        wrap.set_child(theming::roles::GENERIC, Some(&label));
        wrap.set_class_set(
            if cell[1] % 4 == 0 {
                elem_id::SIDEBAR_GROUP_HEADER
            } else {
                elem_id::SIDEBAR_ITEM
            } | if cell[1] == 1 || (cell[1] % 4 == 0 && cell[1] < 28) {
                theming::ClassSet::ACTIVE
            } else {
                theming::ClassSet::empty()
            },
        );

        self.elem.insert_child(wrap.style_elem());

        let button = if cell[1] % 4 == 0 {
            let button = Button::new(self.style_manager);
            // Clear `.BUTTON` and replace with `#SIDEBAR_GROUP_BULLET`
            button.set_class_set(elem_id::SIDEBAR_GROUP_BULLET);

            wrap.set_child(my_roles::BULLET, Some(&button));

            Some(button)
        } else {
            None
        };

        (wrap.view(), Box::new(((wrap, button),)))
    }

    fn range_size(&mut self, line_ty: LineTy, range: Range<u64>, _approx: bool) -> f64 {
        match line_ty {
            LineTy::Row => (range.start..range.end)
                .map(|i| if i % 4 == 0 { 25.0 } else { 20.0 })
                .sum(),

            // `TableFlags::GROW_LAST_COL` expands the column to cover the region.
            // The column needs some width for this flag to work.
            LineTy::Col => (range.end - range.start) as f64,
        }
    }
}
