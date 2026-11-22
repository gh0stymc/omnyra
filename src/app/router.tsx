import { createBrowserRouter } from "react-router";
import { Layout } from "./layout/Layout";
import { DashboardPage } from "../features/dashboard/DashboardPage";
import { PlatformViewPage } from "../features/platform-view/PlatformViewPage";
import { SettingsPage } from "../features/settings/SettingsPage";
import { UploadsPage } from "../features/uploads/UploadsPage";
import { CalendarPage } from "../features/calendar/CalendarPage";
import { IdeasPage } from "../features/ideas/IdeasPage";
import { PublishPage } from "../features/publish/PublishPage";
import { CommunityPage } from "../features/community/CommunityPage";
import { CreationPage } from "../features/creation/CreationPage";
import { SponsorsPage } from "../features/sponsors/SponsorsPage";
import { CompetitorsPage } from "../features/competitors/CompetitorsPage";
import { SecurityPage } from "../features/security/SecurityPage";

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
      { path: "/community", element: <CommunityPage /> },
      { path: "/creation", element: <CreationPage /> },
      { path: "/sponsors", element: <SponsorsPage /> },
      { path: "/competitors", element: <CompetitorsPage /> },
      { path: "/security", element: <SecurityPage /> },
    ],
  },
]);
