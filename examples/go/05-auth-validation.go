// Example 5: Auth and access control
// Tests shatter's ability to reason about string matching, role hierarchies,
// and multi-condition authorization logic.
//
// EXPECTED BRANCHES for AuthorizeRequest (12):
//   1. roles is empty                            -> "denied: no roles"
//   2. active is false                           -> "denied: inactive user"
//   3. resource is empty                         -> "denied: invalid resource"
//   4. action not in {read,write,delete}         -> "denied: invalid action"
//   5. has "superadmin" role                     -> "granted: superadmin"
//   6. is owner + read                           -> "granted: owner-read"
//   7. is owner + write                          -> "granted: owner-write"
//   8. is owner + delete                         -> "granted: owner-delete"
//   9. has "admin" + read                        -> "granted: admin-read"
//  10. has "admin" + write                       -> "granted: admin-write"
//  11. has "admin" + delete                      -> "denied: admin-no-delete"
//  12. has "viewer" + read                       -> "granted: viewer"
//  13. has "viewer" + non-read                   -> "denied: viewer-readonly"
//  14. no matching role                          -> "denied: no matching role"

package examples

// User represents an authenticated user with roles.
type User struct {
	ID     string
	Roles  []string
	Active bool
}

// AuthorizeRequest checks whether a user can perform an action on a resource.
// The priority ordering (superadmin > owner > admin > viewer) requires the
// solver to understand check precedence.
func AuthorizeRequest(user User, resource string, resourceOwnerID string, action string) string {
	if len(user.Roles) == 0 {
		return "denied: no roles"
	}

	if !user.Active {
		return "denied: inactive user"
	}

	if resource == "" {
		return "denied: invalid resource"
	}

	validActions := map[string]bool{"read": true, "write": true, "delete": true}
	if !validActions[action] {
		return "denied: invalid action"
	}

	if contains(user.Roles, "superadmin") {
		return "granted: superadmin"
	}

	isOwner := user.ID == resourceOwnerID

	if isOwner {
		switch action {
		case "read":
			return "granted: owner-read"
		case "write":
			return "granted: owner-write"
		case "delete":
			return "granted: owner-delete"
		}
	}

	if contains(user.Roles, "admin") {
		switch action {
		case "read":
			return "granted: admin-read"
		case "write":
			return "granted: admin-write"
		default:
			return "denied: admin-no-delete"
		}
	}

	if contains(user.Roles, "viewer") {
		if action == "read" {
			return "granted: viewer"
		}
		return "denied: viewer-readonly"
	}

	return "denied: no matching role"
}

func contains(slice []string, item string) bool {
	for _, s := range slice {
		if s == item {
			return true
		}
	}
	return false
}
