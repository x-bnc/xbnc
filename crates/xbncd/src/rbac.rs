//! Role-based access control.
//!
//! Five roles, from most to least privileged. Permissions are checked by the
//! API; the IRC side only needs to know whether a user is enabled.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Can do everything, including system settings and managing admins.
    Superadmin = 5,
    /// Manages users (except superadmins), all networks and security.
    Admin = 4,
    /// Read-only view of users and audit log; can suspend regular users.
    Moderator = 3,
    /// Manages their own networks, channels and history.
    User = 2,
    /// Uses networks set up by an administrator; can manage channels and
    /// history but cannot add, edit or delete networks.
    Limited = 1,
}

/// Actions that need a permission check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perm {
    EditSettings,
    ViewSystem,
    ViewUsers,
    ManageUsers,
    SuspendUsers,
    ViewAudit,
    ManageSecurity,
    ManageOwnNetworks,
    ManageAllNetworks,
}

impl Role {
    pub const ALL: [Role; 5] = [
        Role::Superadmin,
        Role::Admin,
        Role::Moderator,
        Role::User,
        Role::Limited,
    ];

    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "superadmin" => Some(Role::Superadmin),
            "admin" => Some(Role::Admin),
            "moderator" => Some(Role::Moderator),
            "user" => Some(Role::User),
            "limited" => Some(Role::Limited),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Superadmin => "superadmin",
            Role::Admin => "admin",
            Role::Moderator => "moderator",
            Role::User => "user",
            Role::Limited => "limited",
        }
    }

    /// Unknown stored values are treated as the least privileged role.
    pub fn from_db(s: &str) -> Role {
        Role::parse(s).unwrap_or(Role::Limited)
    }

    pub fn can(&self, p: Perm) -> bool {
        use Perm::*;
        let min = match p {
            EditSettings => Role::Superadmin,
            ViewSystem | ManageUsers | ManageSecurity | ManageAllNetworks => Role::Admin,
            ViewUsers | SuspendUsers | ViewAudit => Role::Moderator,
            ManageOwnNetworks => Role::User,
        };
        *self >= min
    }

    /// Whether `self` may create, edit or delete an account that has (or
    /// will get) role `target`. Only a superadmin can touch admins and
    /// superadmins; admins manage the roles below them.
    pub fn can_manage_role(&self, target: Role) -> bool {
        match self {
            Role::Superadmin => true,
            Role::Admin => target < Role::Admin,
            _ => false,
        }
    }

    /// Moderators can suspend/reactivate users and limited users only.
    pub fn can_suspend(&self, target: Role) -> bool {
        self.can_manage_role(target) || (*self == Role::Moderator && target <= Role::User)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_and_parsing() {
        assert!(Role::Superadmin > Role::Admin && Role::User > Role::Limited);
        for r in Role::ALL {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
        assert_eq!(Role::from_db("weird"), Role::Limited);
    }

    #[test]
    fn permission_matrix() {
        assert!(Role::Superadmin.can(Perm::EditSettings));
        assert!(!Role::Admin.can(Perm::EditSettings));
        assert!(Role::Admin.can(Perm::ManageUsers));
        assert!(!Role::Moderator.can(Perm::ManageUsers));
        assert!(Role::Moderator.can(Perm::ViewAudit));
        assert!(Role::User.can(Perm::ManageOwnNetworks));
        assert!(!Role::Limited.can(Perm::ManageOwnNetworks));
        assert!(!Role::User.can(Perm::ViewUsers));
    }

    #[test]
    fn role_management() {
        assert!(Role::Superadmin.can_manage_role(Role::Superadmin));
        assert!(!Role::Admin.can_manage_role(Role::Admin));
        assert!(Role::Admin.can_manage_role(Role::Moderator));
        assert!(!Role::Moderator.can_manage_role(Role::User));
        assert!(Role::Moderator.can_suspend(Role::User));
        assert!(!Role::Moderator.can_suspend(Role::Moderator));
    }
}
