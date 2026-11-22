import { createBrowserRouter } from "react-router";
import { Layout } from "./layout/Layout";
import { DashboardPage } from "../features/dashboard/DashboardPage";
import { PlatformViewPage } from "../features/platform-view/PlatformViewPage";
import { SettingsPage } from "../features/settings/SettingsPage";
import { UploadsPage } from "../features/uploads/UploadsPage";
import { CalendarPage } from "../features/calendar/CalendarPage";
import { IdeasPage } from "../features/ideas/IdeasPage";
import { PublishPage } from "../features/publish/PublishPage";
import { PlaceholderPage } from "../features/placeholder/PlaceholderPage";

export const router = createBrowserRouter([
  {
    element: <Layout />,
    children: [
      { path: "/", element: <DashboardPage /> },
      { path: "/platform/:platform/:accountId", element: <PlatformViewPage /> },
      { path: "/settings", element: <SettingsPage /> },
      { path: "/uploads", element: <UploadsPage /> },
      { path: "/publish", element: <PublishPage /> },
      { path: "/calendar", element: <CalendarPage /> },
      { path: "/ideas", element: <IdeasPage /> },
      { path: "/community", element: <PlaceholderPage section="community" /> },
      { path: "/creation", element: <PlaceholderPage section="creation" /> },
      { path: "/sponsors", element: <PlaceholderPage section="sponsors" /> },
      { path: "/competitors", element: <PlaceholderPage section="competitors" /> },
      { path: "/security", element: <PlaceholderPage section="security" /> },
    ],
  },
]);
