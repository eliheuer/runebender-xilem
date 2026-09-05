// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Service core's live document mailbox through Xilem's application messages.

use crate::Workspace;

/// Pumps the mailbox on the UI thread; socket workers never touch font data.
pub(crate) fn with_live<V: xilem::WidgetView<Workspace>>(
    view: V,
) -> impl xilem::WidgetView<Workspace> + use<V> {
    xilem::core::fork(
        view,
        xilem::view::task_raw(
            |proxy: xilem::core::MessageProxy<()>, _: &mut Workspace| async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                    if proxy.message(()).is_err() {
                        return;
                    }
                }
            },
            |app: &mut Workspace, ()| {
                let request = app.live.as_ref().and_then(|server| server.try_recv());
                if let Some(request) = request {
                    request.respond(|call| {
                        runebender_core::document::live::call(
                            &mut app.font.project,
                            &call.name,
                            &call.arguments,
                        )
                    });
                    app.modified |= app.font.project.masters.iter().any(|master| master.dirty);
                    app.refresh_proposals();
                }
            },
        ),
    )
}
