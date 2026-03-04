// Example 3: Object parameter with field access in conditions
// Tests shatter's ability to generate structured inputs and reason about field values.

interface UserProfile {
    name: string;
    age: number;
    isVerified: boolean;
    role: string;
}

// EXPECTED BRANCHES:
//   1. age < 0                        → throws Error("invalid age")
//   2. age < 13                       → "child"
//   3. age < 18                       → "teen"
//   4. age >= 18, isVerified, admin   → "admin"
//   5. age >= 18, isVerified, !admin  → "verified-user"
//   6. age >= 18, !isVerified         → "unverified-user"

export function categorizeUser(user: UserProfile): string {
    if (user.age < 0) {
        throw new Error("invalid age");
    }
    if (user.age < 13) {
        return "child";
    }
    if (user.age < 18) {
        return "teen";
    }
    if (user.isVerified) {
        if (user.role === "admin") {
            return "admin";
        }
        return "verified-user";
    }
    return "unverified-user";
}

interface Rectangle {
    width: number;
    height: number;
}

// EXPECTED BRANCHES:
//   1. width <= 0 OR height <= 0  → throws Error("non-positive dimension")
//   2. width === height            → "square"
//   3. area > 10000                → "large-rectangle"
//   4. area <= 10000               → "small-rectangle"

export function describeRectangle(rect: Rectangle): string {
    if (rect.width <= 0 || rect.height <= 0) {
        throw new Error("non-positive dimension");
    }
    if (rect.width === rect.height) {
        return "square";
    }
    const area = rect.width * rect.height;
    if (area > 10000) {
        return "large-rectangle";
    }
    return "small-rectangle";
}
