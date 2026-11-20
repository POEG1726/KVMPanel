import { Route, Routes } from 'react-router-dom';
import { ConsolePage } from './pages/ConsolePage';
import { LoginPage } from './pages/LoginPage';
import { WorkspacePage } from './pages/WorkspacePage';
import { RequireAuth } from './session/RequireAuth';

export function App() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route element={<RequireAuth />}>
        <Route path="/" element={<ConsolePage />} />
        <Route path="/workspace" element={<WorkspacePage />} />
      </Route>
    </Routes>
  );
}
