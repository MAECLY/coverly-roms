import React from 'react';
import ReactDOM from 'react-dom/client';

import { Root } from './Root';
import './styles/globals.css';

const root = ReactDOM.createRoot(document.getElementById('root')!);

// Dev-only primitive gallery at `/?ui=gallery`. `import.meta.env.DEV` is a
// build-time `false` in production, so the branch and its chunk are dropped.
if (
  import.meta.env.DEV &&
  new URLSearchParams(window.location.search).get('ui') === 'gallery'
) {
  void import('./dev/UiGallery').then(({ UiGallery }) =>
    root.render(
      <React.StrictMode>
        <UiGallery />
      </React.StrictMode>,
    ),
  );
} else {
  root.render(
    <React.StrictMode>
      <Root />
    </React.StrictMode>,
  );
}
