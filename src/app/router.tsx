import { createBrowserRouter } from "react-router";
import { Layout } from "./layout/Layout";
import { DashboardPage } from "../features/dashboard/DashboardPage";
import { PlatformViewPage } from "../features/platform-view/PlatformViewPage";
import { SettingsPage } from "../features/settings/SettingsPage";
import { PlaceholderPage } from "../features/placeholder/PlaceholderPage";

export const router = createBrowserRouter([
  {
    element: <Layout />,
    children: [
      { path: "/", element: <DashboardPage /> },
      { path: "/platform/:platform/:accountId", element: <PlatformViewPage /> },
      { path: "/settings", element: <SettingsPage /> },
      { path: "/calendar", element: <PlaceholderPage section="calendar" /> },
      { path: "/community", element: <PlaceholderPage section="community" /> },
      { path: "/creation", element: <PlaceholderPage section="creation" /> },
      { path: "/sponsors", element: <PlaceholderPage section="sponsors" /> },
      { path: "/competitors", element: <PlaceholderPage section="competitors" /> },
      { path: "/security", element: <PlaceholderPage section="security" /> },
    ],
  },
]);
