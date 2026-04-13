import React from 'react';

export const NavbarLogo = ({ height = 36, onClick }) => (
  <div
    onClick={onClick}
    style={{ display: 'inline-flex', alignItems: 'center', cursor: onClick ? 'pointer' : 'default' }}
    role={onClick ? 'button' : undefined}
    tabIndex={onClick ? 0 : undefined}
  >
    <img src="./assets/logos/logo-navbar.svg" alt="ClearMemoryAI" height={height} style={{ display: 'block' }} />
  </div>
);

export const FooterLogo = ({ height = 44 }) => (
  <div style={{ display: 'inline-flex', alignItems: 'center' }}>
    <img src="./assets/logos/logo-footer.svg" alt="ClearMemoryAI — Total recall. Zero confusion." height={height} style={{ display: 'block' }} />
  </div>
);
