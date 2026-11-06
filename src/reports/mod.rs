pub mod model;
pub mod notify;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use garde::Error;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState, ClientAddr,
    admin::set_user_disabled,
    auth::AdminUser,
    auth::backend::find_user,
    error::{ApiJson, ApiPath, ApiQuery, AppError},
    guest,
    links::{
        PageQuery,
        model::{delete_link, find_link, set_blocked},
        paginate,
        slug::is_well_formed_slug,
    },
    validate::Id,
};
use model::{Effects, NewReport, close, find, insert, list, open_count, open_ids, reopen};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/reports", post(create))
        .route("/admin/reports", get(list_reports))
        .route("/admin/reports/count", get(count))
        .route("/admin/reports/{id}/resolve", post(resolve))
        .route("/admin/reports/{id}/reopen", post(reopen_report))
}

fn report_status(s: &str, _: &()) -> garde::Result {
    if matches!(s, "open" | "actioned" | "dismissed") {
        Ok(())
    } else {
        Err(Error::new(
            "status must be one of open, actioned, dismissed",
        ))
    }
}

#[derive(Deserialize, garde::Validate)]
struct ReportsQuery {
    #[garde(inner(custom(report_status)))]
    status: Option<String>,
}

async fn list_reports(
    State(s): State<AppState>,
    AdminUser(_): AdminUser,
    ApiQuery(f): ApiQuery<ReportsQuery>,
    ApiQuery(q): ApiQuery<PageQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit();
    let reports = list(
        &s.db,
        f.status.as_deref().unwrap_or("open"),
        q.query.as_deref(),
        q.cursor.map(|c| c.0),
        i64::from(limit) + 1,
    )
    .await?;
    let (reports, next) = paginate(reports, limit, |r| r.id);
    Ok(Json(json!({ "reports": reports, "next_cursor": next })))
}

async fn count(
    State(s): State<AppState>,
    AdminUser(_): AdminUser,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({ "open": open_count(&s.db).await? })))
}

#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
struct ResolveBody {
    action: Action,
}

#[derive(Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Action {
    Block,
    Delete,
    DisableOwner,
    Dismiss,
}

async fn resolve(
    State(s): State<AppState>,
    AdminUser(me): AdminUser,
    ApiPath(Id(id)): ApiPath<Id>,
    ApiJson(body): ApiJson<ResolveBody>,
) -> Result<StatusCode, AppError> {
    let model::Found {
        link_id,
        status,
        owner,
        ..
    } = find(&s.db, id).await?.ok_or(AppError::NotFound)?;
    if status != "open" {
        return Err(AppError::Conflict(
            "this report was already resolved. Refresh to see its outcome".into(),
        ));
    }
    if body.action == Action::Dismiss {
        if !close(&s.db, &[id], "dismissed", None, Effects::default(), me.id).await? {
            return Err(AppError::Conflict(
                "this report was already resolved. Refresh to see its outcome".into(),
            ));
        }
        tracing::info!(admin_id = me.id, report_id = id, "report dismissed");
        return Ok(StatusCode::NO_CONTENT);
    }
    let link_id = link_id.ok_or_else(|| {
        AppError::Unprocessable("this link no longer exists. Dismiss the report instead".into())
    })?;
    // What was already true, so a reopen only undoes what this action changed.
    let mut effects = Effects {
        blocked_link: !find_link(&s.db, link_id).await?.is_some_and(|l| l.blocked),
        disabled_owner: false,
    };
    let action = match body.action {
        Action::Delete => "deleted",
        Action::Block => "blocked",
        _ => {
            let owner = owner.ok_or_else(|| {
                AppError::Unprocessable("guest links have no owner to disable".into())
            })?;
            if owner == me.id {
                return Err(AppError::Conflict(
                    "you can't disable your own account".into(),
                ));
            }
            effects.disabled_owner = !find_user(&s.db, owner).await?.is_some_and(|u| u.disabled);
            set_user_disabled(&s, owner, true).await?;
            "owner_disabled"
        }
    };
    let ids = open_ids(&s.db, link_id).await?;
    let slug = if action == "deleted" {
        // Blocking first makes delete_link retire the slug.
        set_blocked(&s.db, link_id, true).await?;
        delete_link(&s.db, link_id).await?
    } else {
        set_blocked(&s.db, link_id, true).await?
    }
    .ok_or_else(|| {
        AppError::Unprocessable("this link no longer exists. Dismiss the report instead".into())
    })?;
    s.cache.evict(&slug);
    close(&s.db, &ids, "actioned", Some(action), effects, me.id).await?;
    tracing::info!(
        admin_id = me.id,
        report_id = id,
        link_id,
        slug,
        action,
        closed = ids.len(),
        "report actioned"
    );
    Ok(StatusCode::NO_CONTENT)
}

/// Undoes a resolution: a dismissed report goes back to open; an actioned one also reverses its
/// action, as far as it changed things (unblocks the link, re-enables the owner). Deleted links can't come back.
async fn reopen_report(
    State(s): State<AppState>,
    AdminUser(me): AdminUser,
    ApiPath(Id(id)): ApiPath<Id>,
) -> Result<Json<Value>, AppError> {
    let r = find(&s.db, id).await?.ok_or(AppError::NotFound)?;
    if r.status == "open" {
        return Err(AppError::Conflict("this report is already open".into()));
    }
    if r.action.as_deref() == Some("deleted") {
        return Err(AppError::Conflict(
            "the link was deleted and can't be restored, so this report can't be reopened".into(),
        ));
    }
    let gone =
        || AppError::Conflict("this link no longer exists, so there's nothing to review".into());
    let link_id = r.link_id.ok_or_else(gone)?;
    let reopened = reopen(&s.db, id, &r).await?;
    if reopened == 0 {
        return Err(AppError::Conflict(
            "this report changed meanwhile. Refresh to see it".into(),
        ));
    }
    if r.status == "actioned" {
        if let (true, Some(owner)) = (r.disabled_owner, r.owner) {
            set_user_disabled(&s, owner, false).await?;
        }
        if r.blocked_link {
            let slug = set_blocked(&s.db, link_id, false).await?.ok_or_else(gone)?;
            s.cache.evict(&slug);
        }
    }
    tracing::info!(
        admin_id = me.id,
        report_id = id,
        link_id,
        undone = r.action.as_deref(),
        reopened,
        "report reopened"
    );
    Ok(Json(json!({ "reopened": reopened })))
}

async fn create(
    State(s): State<AppState>,
    ClientAddr(ip): ClientAddr,
    ApiJson(report): ApiJson<NewReport>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    guest::check(&s.report_limiter, ip)?;
    if !is_well_formed_slug(&report.slug) || !insert(&s.db, &report.slug, &report).await? {
        return Err(AppError::Unprocessable(
            "slug doesn't match any link".into(),
        ));
    }
    tracing::info!(slug = report.slug, reason = %report.reason, "report filed");
    Ok((StatusCode::CREATED, Json(serde_json::json!({}))))
}
