use mockall::automock;

#[tokio::main]
async fn main() {
    let foo = FooImpl {};
    let baz = BazImpl {};
    baz.baz(foo).await;
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn test_foo() {
        let captured_update_fn: Arc<Mutex<Option<Box<dyn FnOnce(Zed) -> Zed + Send + 'static>>>> =
            Arc::new(Mutex::new(None));
        let captured_update_fn_clone = Arc::clone(&captured_update_fn);

        let mut mock_foo = MockFoo::new();
        mock_foo
            .expect_bar()
            .times(1)
            .withf(
                move |update_fn: &Box<dyn FnOnce(Zed) -> Zed + Send + 'static>| {
                    // let mut captured = captured_update_fn_clone.lock().unwrap();
                    // *captured = Some(update_fn.clone().to_owned());
                    true
                },
            )
            .return_const(());

        let baz = BazImpl {};
        baz.baz(mock_foo).await;

        assert!(captured_update_fn.lock().unwrap().is_some());
    }
}

struct Zed;

#[automock]
trait Foo {
    async fn bar<F>(&self, update_fn: F)
    where
        F: FnOnce(Zed) -> Zed + Send + 'static;
}

struct FooImpl;

impl Foo for FooImpl {
    async fn bar<F>(&self, update_fn: F)
    where
        F: FnOnce(Zed) -> Zed + Send + 'static,
    {
        update_fn(Zed {});
    }
}

struct BazImpl;

impl BazImpl {
    async fn baz<F: Foo>(self, f: F) {
        f.bar(|zed| zed).await;
    }
}
