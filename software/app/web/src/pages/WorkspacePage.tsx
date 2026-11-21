import { lazy, Suspense } from 'react';
import { ToastHost } from '../ui';

export const Workspace = lazy(() => import('../workspace/Workspace'));

export function WorkspacePage() {
  return <><Suspense fallback={null}><Workspace mode="tab" /></Suspense><ToastHost /></>;
}
