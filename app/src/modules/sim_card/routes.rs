use std::str::FromStr;

use super::dto::{self, ListSimCardsDto};
use crate::{
    database::{self, error::DbError},
    modules::{
        auth::{self, middleware::AclLayer},
        common::{
            dto::{Pagination, PaginationResult},
            extractors::{DbConnection, OrganizationId, ValidatedJson, ValidatedQuery},
            responses::{internal_error_res, SimpleError},
        },
    },
    server::controller::AppState,
};
use axum::{
    extract::Path,
    routing::{delete, get, put},
    Json, Router,
};
use entity::{sim_card, vehicle_tracker};
use http::StatusCode;
use migration::Expr;
use sea_orm::{sea_query::extension::postgres::PgExpr, QuerySelect};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QueryTrait};
use shared::{Permission, TrackerModel};

pub fn create_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(list_sim_cards))
        .route("/:sim_card_id", delete(delete_sim_card))
        .layer(AclLayer::new(vec![Permission::DeleteSimCard]))
        .route("/:sim_card_id/tracker", put(set_sim_card_tracker))
        .layer(AclLayer::new(vec![Permission::UpdateTracker]))
        .layer(axum::middleware::from_fn_with_state(
            state,
            auth::middleware::require_user,
        ))
}

/// Sets a sim card tracker
///
/// Required permissions: UPDATE_SIM_CARD
#[utoipa::path(
    put,
    tag = "sim-card",
    path = "/sim-card/{sim_card_id}/tracker",
    security(("session_id" = [])),
    params(
        ("sim_card_id" = u128, Path, description = "id of the sim card to associate to the tracker"),
    ),
    request_body(content = SetSimCardTrackerDto),
    responses(
        (
            status = OK,
            description = "success message",
            body = String,
            content_type = "application/json",
            example = json!("sim card tracker set successfully"),
        ),
        (
            status = BAD_REQUEST,
            description = "sim card <id> is already has a tracker",
            body = SimpleError,
        ),
    ),
)]
pub async fn set_sim_card_tracker(
    Path(sim_card_id): Path<i32>,
    OrganizationId(org_id): OrganizationId,
    DbConnection(db): DbConnection,
    ValidatedJson(payload): ValidatedJson<dto::SetSimCardTrackerDto>,
) -> Result<Json<String>, (StatusCode, SimpleError)> {
    // here we can unwrap tracker_id because its guaranteed
    // by the DTO validation to be `Some`
    let tracker_id_or_none = payload.tracker_id.ok_or(internal_error_res())?;

    let sim_card = sim_card::Entity::find_by_id(sim_card_id)
        .filter(sim_card::Column::OrganizationId.eq(org_id))
        .one(&db)
        .await
        .map_err(DbError::from)?
        .ok_or((
            StatusCode::NOT_FOUND,
            SimpleError::from("sim card not found"),
        ))?;

    if let Some(new_tracker_id) = tracker_id_or_none {
        let tracker = vehicle_tracker::Entity::find_by_id(new_tracker_id)
            .filter(vehicle_tracker::Column::OrganizationId.eq(org_id))
            .one(&db)
            .await
            .map_err(DbError::from)?
            .ok_or((
                StatusCode::NOT_FOUND,
                SimpleError::from("tracker not found"),
            ))?;

        if sim_card.tracker_id == Some(new_tracker_id) {
            let success_msg = format!(
                "sim card is already associated with tracker: {}",
                new_tracker_id
            );
            return Ok(Json(String::from(success_msg)));
        }

        // TODO: make tracker.model be a enum that maps to `TrackerModel` so we dont need this crap
        // also make it a enum on the database level to avoid bad inserts.
        // https://www.sea-ql.org/SeaORM/docs/generate-entity/enumeration/
        let sim_card_slots_for_new_tracker = TrackerModel::from_str(&tracker.model)
            .or(Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                SimpleError::from("invalid tracker model"),
            )))?
            .get_info()
            .sim_card_slots;

        let sim_cards_associated_with_tracker: i64 = sim_card::Entity::find()
            .select_only()
            .column_as(sim_card::Column::Id.count(), "count")
            .filter(sim_card::Column::TrackerId.eq(new_tracker_id))
            .into_tuple()
            .one(&db)
            .await
            .map_err(DbError::from)?
            .unwrap_or(0);

        if sim_cards_associated_with_tracker + 1 > sim_card_slots_for_new_tracker.into() {
            let err_msg = "associating the sim card with the tracker would overflow the SIM slots for the tracker model";
            return Err((StatusCode::BAD_REQUEST, SimpleError::from(err_msg)));
        }
    }

    sim_card::Entity::update_many()
        .col_expr(sim_card::Column::TrackerId, Expr::value(tracker_id_or_none))
        .filter(sim_card::Column::Id.eq(sim_card_id))
        .filter(sim_card::Column::OrganizationId.eq(org_id))
        .exec(&db)
        .await
        .map_err(DbError::from)?;

    Ok(Json(String::from("sim card tracker set successfully")))
}

/// Deletes a SIM card
#[utoipa::path(
    delete,
    tag = "sim-card",
    path = "/sim-card/{sim_card_id}",
    security(("session_id" = [])),
    params(
        ("sim_card_id" = u128, Path, description = "id of the SIM card to delete"),
    ),
    responses(
        (
            status = OK,
            description = "success message",
            body = String,
            content_type = "application/json",
            example = json!("SIM card deleted successfully"),
        ),
    ),
)]
pub async fn delete_sim_card(
    Path(sim_card_id): Path<i32>,
    OrganizationId(org_id): OrganizationId,
    DbConnection(db): DbConnection,
) -> Result<Json<String>, (StatusCode, SimpleError)> {
    let delete_result = sim_card::Entity::delete_many()
        .filter(sim_card::Column::Id.eq(sim_card_id))
        .filter(sim_card::Column::OrganizationId.eq(org_id))
        .exec(&db)
        .await
        .map_err(DbError::from)?;

    if delete_result.rows_affected < 1 {
        let err_msg = "SIM card does not exist or does not belong to the request user organization";
        Err((StatusCode::BAD_REQUEST, SimpleError::from(err_msg)))
    } else {
        Ok(Json(String::from("sim card deleted successfully")))
    }
}

/// Lists the SIM cards that belong to the same org as the request user
#[utoipa::path(
    get,
    tag = "sim-card",
    path = "/sim-card",
    security(("session_id" = [])),
    params(
        Pagination,
        ListSimCardsDto
    ),
    responses(
        (
            status = OK,
            description = "paginated list of SIM cards",
            content_type = "application/json",
            body = PaginatedSimCard,
        ),
    ),
)]
pub async fn list_sim_cards(
    ValidatedQuery(pagination): ValidatedQuery<Pagination>,
    ValidatedQuery(filter): ValidatedQuery<ListSimCardsDto>,
    OrganizationId(org_id): OrganizationId,
    DbConnection(db): DbConnection,
) -> Result<Json<PaginationResult<entity::sim_card::Model>>, (StatusCode, SimpleError)> {
    let db_query = sim_card::Entity::find()
        .filter(sim_card::Column::OrganizationId.eq(org_id))
        .apply_if(filter.with_associated_tracker, |query, with_vehicle| {
            if with_vehicle {
                query.filter(sim_card::Column::TrackerId.is_not_null())
            } else {
                query.filter(sim_card::Column::TrackerId.is_null())
            }
        })
        .apply_if(filter.phone_number, |query, phone| {
            if phone != "" {
                let col = Expr::col((sim_card::Entity, sim_card::Column::PhoneNumber));
                query.filter(col.ilike(format!("%{}%", phone)))
            } else {
                query
            }
        })
        .order_by_asc(sim_card::Column::Id)
        .paginate(&db, pagination.page_size);

    let result = database::helpers::paginated_query_to_pagination_result(db_query, pagination)
        .await
        .map_err(DbError::from)?;

    Ok(Json(result))
}