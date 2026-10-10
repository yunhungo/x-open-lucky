/**
 * @created 2026-10-11
 * @description 初始化路由、查询缓存与全局反馈。
 * @author yunhungo
 */
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import {
  QueryCache,
  QueryClient,
  QueryClientProvider,
} from '@tanstack/react-query';
import { App } from '@/components/App/App';
import { ApiError } from '@/lib/api';
import '@/styles/tailwind.css';
import '@/styles/global.scss';
const client = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, staleTime: 3000, refetchOnWindowFocus: true },
  },
  queryCache: new QueryCache({
    onError: (error, query) => {
      if (
        error instanceof ApiError &&
        error.status === 401 &&
        query.queryKey[0] !== 'me'
      )
        void client.invalidateQueries({ queryKey: ['me'] });
    },
  }),
});
const root = document.getElementById('root');
if (!root) throw new Error('Root element is missing');
createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={client}>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </QueryClientProvider>
  </StrictMode>,
);
