use axum::{
    Json, Router,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        // `POST /users` goes to `create_user`
        .route("/users", post(create_user));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    let _ = axum::serve(listener, app).await;
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn create_user(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateUser` type
    Json(payload): Json<CreateUser>,
) -> (StatusCode, Json<User>) {
    // insert your application logic here
    let user = User {
        id: 1337,
        username: payload.username,
    };

    // this will be converted into a JSON response
    // with a status code of `201 Created`
    (StatusCode::CREATED, Json(user))
}

// the input to our `create_user` handler
#[derive(Deserialize, Serialize)]
struct CreateUser {
    username: String,
}

// the output to our `create_user` handler
#[derive(Serialize, Deserialize)]
struct User {
    id: u64,
    username: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_root() {
        // 构建与主函数相同的路由
        let app = Router::new()
            .route("/", get(root))
            .route("/users", post(create_user));

        // 使用 axum::test 发送 GET 请求
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        // 断言状态码为 200 OK
        assert_eq!(response.status(), StatusCode::OK);

        // // 读取响应体并断言内容
        // let body = response.into_body().collect().await.unwrap().to_bytes();
        // assert_eq!(&body[..], b"Hello, World!");
        // 读取响应体并转换为字符串断言内容
        let body = String::from_utf8(
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert_eq!(body, "Hello, World!");
    }

    #[tokio::test]
    async fn test_create_user() {
        let app = Router::new()
            .route("/", get(root))
            .route("/users", post(create_user));

        // 构造请求的 JSON 数据
        let payload = CreateUser {
            username: "test_user".to_string(),
        };

        // 使用 axum::test 发送 POST 请求
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/users")
                    .header("Content-Type", "application/json")
                    .body::<Body>(serde_json::to_string(&payload).unwrap().into())
                    .unwrap(),
            )
            .await
            .unwrap();

        // 断言状态码为 201 Created
        assert_eq!(response.status(), StatusCode::CREATED);

        // 读取并解析响应体
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let user: User = serde_json::from_slice(&body).unwrap();

        // 断言返回的数据符合预期
        assert_eq!(user.id, 1337);
        assert_eq!(user.username, "test_user");
    }
}
