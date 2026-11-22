import { lazy, type ComponentType } from "react";
import { createBrowserRouter } from "react-router";
import { Layout } from "./layout/Layout";
import { DashboardPage } from "../features/dashboard/DashboardPage";

/** Code-split page (REVIEW P1): loaded the first time its route opens. */
function page<K extends string>(load: () => Promise<Record<K, ComponentType>>, name: K) {
  const C: ComponentType = lazy(() => load().then((m) => ({ default: m[name] as ComponentType })));
  return <C />;
}

export const router = createBrowserRouter([
  {
    element: <Layout />,
    children: [
      { path: "/", element: <DashboardPage /> },
      {
        path: "/platform/:platform/:accountId",
        element: page(
          () => import("../features/platform-view/PlatformViewPage"),
          "PlatformViewPage",
        ),
      },
      {
        path: "/settings",
        element: page(() => import("../features/settings/SettingsPage"), "SettingsPage"),
      },
      {
        path: "/uploads",
        element: page(() => import("../features/uploads/UploadsPage"), "UploadsPage"),
      },
      {
        path: "/publish",
        element: page(() => import("../features/publish/PublishPage"), "PublishPage"),
      },
      {
        path: "/calendar",
        element: page(() => import("../features/calendar/CalendarPage"), "CalendarPage"),
      },
      { path: "/ideas", element: page(() => import("../features/ideas/IdeasPage"), "IdeasPage") },
      {
        path: "/community",
        element: page(() => import("../features/community/CommunityPage"), "CommunityPage"),
      },
      {
        path: "/creation",
        element: page(() => import("../features/creation/CreationPage"), "CreationPage"),
      },
      {
        path: "/sponsors",
        element: page(() => import("../features/sponsors/SponsorsPage"), "SponsorsPage"),
      },
      {
        path: "/competitors",
        element: page(() => import("../features/competitors/CompetitorsPage"), "CompetitorsPage"),
      },
      {
        path: "/security",
        element: page(() => import("../features/security/SecurityPage"), "SecurityPage"),
      },
    ],
  },
]);
