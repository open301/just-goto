import { createRoot } from 'react-dom/client';
import { App } from './App';
import './style.css';

// Mount the app into the container provided by index.html.
const root = document.getElementById('root');
if (!root) throw new Error('The application root is missing.');
createRoot(root).render(<App />);
