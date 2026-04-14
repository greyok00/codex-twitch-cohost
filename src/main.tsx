import { createRoot } from 'react-dom/client';
import './lib/styles/global.css';
import App from './App';

createRoot(document.getElementById('app') as HTMLElement).render(<App />);
