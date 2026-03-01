// Example 6: Authenticate User via Trait-Based Data Store
// Tests shatter's ability to handle trait-based external dependencies.
// The DataStore trait allows mocking for testing without real I/O.
//
// EXPECTED BRANCHES (5):
//   1. username is empty              -> "error: empty username"
//   2. password is empty              -> "error: empty password"
//   3. user not found in store        -> "error: user not found"
//   4. password does not match        -> "error: invalid password"
//   5. user found and password matches -> "ok: welcome {username}"
//
// DIFFICULTY: Hard. Requires generating mock implementations of a trait
// or understanding that the data store behavior affects control flow.

pub trait DataStore {
    fn lookup_user(&self, username: &str) -> Option<StoredUser>;
}

pub struct StoredUser {
    pub username: String,
    pub password_hash: String,
}

pub fn authenticate(store: &dyn DataStore, username: &str, password: &str) -> String {
    if username.is_empty() {
        return "error: empty username".to_string();
    }
    if password.is_empty() {
        return "error: empty password".to_string();
    }

    match store.lookup_user(username) {
        None => "error: user not found".to_string(),
        Some(user) => {
            if user.password_hash == password {
                format!("ok: welcome {username}")
            } else {
                "error: invalid password".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockStore {
        users: Vec<StoredUser>,
    }

    impl DataStore for MockStore {
        fn lookup_user(&self, username: &str) -> Option<StoredUser> {
            self.users.iter().find(|u| u.username == username).map(|u| StoredUser {
                username: u.username.clone(),
                password_hash: u.password_hash.clone(),
            })
        }
    }

    fn store_with_alice() -> MockStore {
        MockStore {
            users: vec![StoredUser {
                username: "alice".to_string(),
                password_hash: "secret123".to_string(),
            }],
        }
    }

    #[test]
    fn test_authenticate_empty_username() {
        let store = store_with_alice();
        assert_eq!(authenticate(&store, "", "pass"), "error: empty username");
    }

    #[test]
    fn test_authenticate_empty_password() {
        let store = store_with_alice();
        assert_eq!(authenticate(&store, "alice", ""), "error: empty password");
    }

    #[test]
    fn test_authenticate_user_not_found() {
        let store = store_with_alice();
        assert_eq!(
            authenticate(&store, "bob", "pass"),
            "error: user not found"
        );
    }

    #[test]
    fn test_authenticate_invalid_password() {
        let store = store_with_alice();
        assert_eq!(
            authenticate(&store, "alice", "wrong"),
            "error: invalid password"
        );
    }

    #[test]
    fn test_authenticate_success() {
        let store = store_with_alice();
        assert_eq!(
            authenticate(&store, "alice", "secret123"),
            "ok: welcome alice"
        );
    }
}
