import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'LocalGPT',
  tagline: 'A local AI assistant with persistent markdown memory, autonomous heartbeat tasks, and semantic search. Single binary, no runtime dependencies.',
  // favicon: 'img/favicon.ico',

  url: 'https://localgpt.app',
  baseUrl: '/',

  organizationName: 'localgpt-app',
  projectName: 'localgpt-app',

  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/localgpt-app/localgpt/tree/main/localgpt-app-docusaurus/',
        },
        blog: {
          showReadingTime: true,
          editUrl: 'https://github.com/localgpt-app/localgpt/tree/main/localgpt-app-docusaurus/',
        },
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    colorMode: {
      defaultMode: 'dark',
      disableSwitch: true,
      respectPrefersColorScheme: false,
    },
    image: 'logo/localgpt-logo-dark.svg',
    navbar: {
      title: 'LocalGPT',
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'tutorialSidebar',
          position: 'left',
          label: 'Docs',
        },
        {to: '/blog', label: 'Blog', position: 'left'},
        {
          href: 'https://github.com/localgpt-app/localgpt',
          position: 'right',
          className: 'header-github-link',
          'aria-label': 'GitHub repository',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Documentation',
          items: [
            {
              label: 'Getting Started',
              to: '/docs/intro',
            },
            {
              label: 'CLI Commands',
              to: '/docs/cli-commands',
            },
            {
              label: 'Configuration',
              to: '/docs/configuration',
            },
          ],
        },
        {
          title: 'Features',
          items: [
            {
              label: 'Memory System',
              to: '/docs/memory-system',
            },
            {
              label: 'Heartbeat',
              to: '/docs/heartbeat',
            },
            {
              label: 'HTTP API',
              to: '/docs/http-api',
            },
          ],
        },
        {
          title: 'More',
          items: [
            {
              label: 'Blog',
              to: '/blog',
            },
            {
              label: 'GitHub',
              href: 'https://github.com/localgpt-app/localgpt',
            },
          ],
        },
      ],
      copyright: `Licensed under Apache 2.0`,
    },
    prism: {
      theme: prismThemes.dracula,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['bash', 'toml', 'rust', 'json'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
