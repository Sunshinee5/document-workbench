import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './index.css';

// 应用挂载入口：正式 MVVM 中 UI 只显示状态、只发命令，不解析文档。
const rootElement = document.getElementById('root');

if (rootElement) {
  ReactDOM.createRoot(rootElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}
