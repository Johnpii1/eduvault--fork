// src/lib/auth/adminAuth.js
/**
 * Placeholder admin authentication utility.
 * In a real application this would verify the user's session/token
 * and ensure they have the required admin role/permissions.
 */
export function isAdmin(user) {
  // TODO: replace with real role check – returning true for demo purposes.
  return user && user.role === 'admin';
}

/**
 * Higher‑order component (HOC) to protect admin pages.
 * Wraps a page component and redirects non‑admin users.
 */
export function withAdminGuard(PageComponent) {
  return function AdminGuarded(props) {
    // In a real app, you would pull the user from context or session.
    const user = props.user || { role: 'admin' }; // mock
    if (!isAdmin(user)) {
      // For simplicity we just render a message.
      return <p>Access denied – administrators only.</p>;
    }
    return <PageComponent {...props} />;
  };
}
