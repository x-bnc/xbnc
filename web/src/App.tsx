import { Navigate, Route, Routes } from "react-router-dom";
import { useSession } from "./lib/session";
import { Layout } from "./components/Layout";
import { Loading } from "./components/ui";
import { Login, Setup } from "./pages/Auth";
import { Dashboard } from "./pages/Dashboard";
import { Networks, NewNetwork } from "./pages/Networks";
import { NetworkDetail } from "./pages/NetworkDetail";
import { UsersPage } from "./pages/Users";
import { SecurityPage } from "./pages/Security";
import { SettingsPage } from "./pages/Settings";
import { ProfilePage } from "./pages/Profile";
import { ModulesPage } from "./pages/Modules";
import { PluginsPage } from "./pages/Plugins";
import { ChatPage } from "./pages/Chat";
import { MentionsPage } from "./pages/Mentions";

export function App() {
  const { me, loading, setupNeeded } = useSession();
  if (loading) return <Loading />;
  if (!me) return setupNeeded ? <Setup /> : <Login />;
  const p = me.permissions;
  return (
    <Layout>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/networks" element={<Networks />} />
        <Route path="/networks/new" element={p.manage_own_networks || p.manage_all_networks ? <NewNetwork /> : <Navigate to="/networks" />} />
        <Route path="/networks/:id" element={<NetworkDetail />} />
        <Route path="/modules" element={<ModulesPage />} />
        <Route path="/plugins" element={<PluginsPage />} />
        <Route path="/chat" element={<ChatPage />} />
        <Route path="/mentions" element={<MentionsPage />} />
        <Route path="/users" element={p.view_users ? <UsersPage /> : <Navigate to="/" />} />
        <Route path="/security" element={p.view_audit ? <SecurityPage /> : <Navigate to="/" />} />
        <Route path="/settings" element={p.view_system ? <SettingsPage /> : <Navigate to="/" />} />
        <Route path="/profile" element={<ProfilePage />} />
        <Route path="*" element={<Navigate to="/" />} />
      </Routes>
    </Layout>
  );
}
