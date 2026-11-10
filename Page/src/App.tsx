import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import { App as KonstaApp, Button, Segmented, SegmentedButton } from 'konsta/react'
import { LiquidGlass } from '@sohumsuthar/liquid-glass'
import { MarkGithubIcon } from '@primer/octicons-react'
import {
  ArrowRight,
  BadgeCheck,
  Bell,
  Boxes,
  Captions,
  Check,
  ChevronRight,
  Code2,
  Command,
  Disc3,
  Download,
  Gauge,
  GitFork,
  Languages,
  LayoutGrid,
  Menu,
  MessageSquareText,
  Moon,
  Music2,
  Orbit,
  Palette,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Sun,
  Waves,
  X,
} from 'lucide-react'
import { HashRouter, Link, useLocation } from 'react-router-dom'
import appIcon from '../../resources/icon-dark.ico'
import settingsIcon from '../../resources/in_app/settings/settings.png'
import widgetIcon from '../../resources/in_app/settings/widget.png'
import compactAfterglowEn from './assets/island/compact-afterglow-en.webp'
import compactWanfengZh from './assets/island/compact-wanfeng-zh.webp'
import heroPoster from './assets/media/hero-loop-poster.jpg'
import { GlassFilter } from './components/GlassFilter'
import { ProductDemo } from './components/ProductDemo'
import Aurora from './components/reactbits/Aurora'
import GlareHover from './components/reactbits/GlareHover'
import LogoLoop from './components/reactbits/LogoLoop'
import Magnet from './components/reactbits/Magnet'
import ScrollStack, { ScrollStackItem } from './components/reactbits/ScrollStack'
import ShinyText from './components/reactbits/ShinyText'
import SpotlightCard from './components/reactbits/SpotlightCard'
import StarBorder from './components/reactbits/StarBorder'
import TiltedCard from './components/reactbits/TiltedCard'
import {
  copy,
  DOC_KEYS,
  DOWNLOAD_NIGHTLY,
  DOWNLOAD_RELEASE,
  GITHUB_URL,
  localePath,
  type DemoMode,
  type DocKey,
  type Locale,
} from './content'

const DocumentPage = lazy(() => import('./components/DocumentPage'))
const WinWidgetPage = lazy(() => import('./components/WinWidgetPage'))
const LiquidGlassPage = lazy(() => import('./components/LiquidGlassPage'))
type SiteTheme = 'light' | 'dark'

const THEME_STORAGE_KEY = 'winisland-site-theme'

const systemTheme = (): SiteTheme =>
  window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'

const storedTheme = (): SiteTheme | null => {
  try {
    const value = window.localStorage.getItem(THEME_STORAGE_KEY)
    return value === 'light' || value === 'dark' ? value : null
  } catch {
    return null
  }
}

function useSiteTheme() {
  const [preference, setPreference] = useState<SiteTheme | null>(storedTheme)
  const [system, setSystem] = useState<SiteTheme>(systemTheme)
  const theme = preference ?? system

  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const update = (event: MediaQueryListEvent) => setSystem(event.matches ? 'dark' : 'light')
    media.addEventListener('change', update)
    return () => media.removeEventListener('change', update)
  }, [])

  useLayoutEffect(() => {
    document.documentElement.dataset.theme = theme
    document.documentElement.style.colorScheme = theme
    document.querySelector('meta[name="theme-color"]')?.setAttribute(
      'content',
      theme === 'dark' ? '#0f1013' : '#f5f5f7',
    )
  }, [theme])

  const toggle = useCallback(() => {
    const next = theme === 'dark' ? 'light' : 'dark'
    setPreference(next)
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, next)
    } catch {}
  }, [theme])

  return { theme, toggle }
}

const pageFromPath = (pathname: string) => {
  const cleanPath = pathname.replace(/^\/zh(?=\/|$)/, '') || '/'
  if (cleanPath === '/') return 'home' as const
  if (cleanPath === '/winwidget') return 'winwidget' as const
  if (cleanPath === '/liquid-glass') return 'liquid-glass' as const
  const key = cleanPath.replace(/^\//, '') as DocKey
  return DOC_KEYS.includes(key) ? key : 'not-found'
}

function App() {
  return (
    <HashRouter>
      <Site />
    </HashRouter>
  )
}

function Site() {
  const location = useLocation()
  const { theme, toggle: toggleTheme } = useSiteTheme()
  const locale: Locale = location.pathname.startsWith('/zh') ? 'zh' : 'en'
  const page = pageFromPath(location.pathname)

  useEffect(() => {
    window.scrollTo({ top: 0, behavior: 'instant' })
  }, [page])

  useEffect(() => {
    document.documentElement.lang = locale === 'zh' ? 'zh-CN' : 'en'
    document.title = page === 'winwidget'
      ? 'WinWidget Desktop'
      : page === 'liquid-glass' ? 'Liquid Glass · WinIsland' : 'WinIsland'
  }, [locale, page])

  return (
    <KonstaApp theme="ios" dark={theme === 'dark'} safeAreas={false} className="site-app">
      <GlassFilter />
      <SiteHeader locale={locale} theme={theme} onThemeToggle={toggleTheme} />
      <main>
        <div className="page-transition" key={page}>
          {page === 'home' && <HomePage locale={locale} theme={theme} />}
          {page === 'download' && <DownloadPage locale={locale} />}
          {page === 'winwidget' && (
            <Suspense fallback={<div className="docs-loading page-top section-shell" aria-busy="true"><span /></div>}>
              <WinWidgetPage locale={locale} theme={theme} />
            </Suspense>
          )}
          {page === 'liquid-glass' && (
            <Suspense fallback={<div className="docs-loading page-top section-shell" aria-busy="true"><span /></div>}>
              <LiquidGlassPage locale={locale} theme={theme} />
            </Suspense>
          )}
          {page !== 'home' && page !== 'winwidget' && page !== 'liquid-glass' && page !== 'download' && page !== 'not-found' && (
            <Suspense fallback={<div className="docs-loading page-top section-shell" aria-busy="true"><span /></div>}>
              <DocumentPage key={`${locale}/${page}`} locale={locale} page={page} />
            </Suspense>
          )}
          {page === 'not-found' && <NotFound locale={locale} />}
        </div>
      </main>
      <SiteFooter locale={locale} />
    </KonstaApp>
  )
}

function SiteHeader({
  locale,
  theme,
  onThemeToggle,
}: {
  locale: Locale
  theme: SiteTheme
  onThemeToggle: () => void
}) {
  const [open, setOpen] = useState(false)
  const [indicator, setIndicator] = useState({ x: 0, width: 0, visible: false })
  const headerRef = useRef<HTMLElement>(null)
  const menuButtonRef = useRef<HTMLButtonElement>(null)
  const navRef = useRef<HTMLElement>(null)
  const navLinks = useRef<Array<HTMLAnchorElement | null>>([])
  const location = useLocation()
  const text = copy[locale]
  const otherLocale: Locale = locale === 'en' ? 'zh' : 'en'
  const currentPath = location.pathname.replace(/^\/zh(?=\/|$)/, '') || '/'
  const guidePaths = ['/guide', '/getting-started', '/plugin-dev', '/api-changelog']
  const isGuidePath = guidePaths.includes(currentPath) || currentPath.startsWith('/plugin-dev/')
  const activeIndex = currentPath === '/'
    ? 0
    : currentPath === '/winwidget'
      ? 1
      : currentPath === '/liquid-glass'
        ? 2
        : isGuidePath ? 3 : currentPath === '/changelog' ? 4 : -1
  const themeLabel = theme === 'dark'
    ? locale === 'zh' ? '切换为浅色模式' : 'Switch to light mode'
    : locale === 'zh' ? '切换为深色模式' : 'Switch to dark mode'

  const moveIndicator = useCallback((index: number) => {
    const nav = navRef.current
    const link = navLinks.current[index]
    if (!nav || !link) {
      setIndicator((value) => ({ ...value, visible: false }))
      return
    }

    const navRect = nav.getBoundingClientRect()
    const linkRect = link.getBoundingClientRect()
    setIndicator({ x: linkRect.left - navRect.left, width: linkRect.width, visible: true })
  }, [])

  useEffect(() => setOpen(false), [location.pathname])

  useEffect(() => {
    if (!open) return

    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpen(false)
        menuButtonRef.current?.focus()
      }
    }
    const closeOnOutsidePress = (event: PointerEvent) => {
      if (!headerRef.current?.contains(event.target as Node)) setOpen(false)
    }

    document.addEventListener('keydown', closeOnEscape)
    document.addEventListener('pointerdown', closeOnOutsidePress)
    const focusFrame = window.requestAnimationFrame(() => {
      if (window.matchMedia('(max-width: 820px)').matches) navLinks.current[0]?.focus()
    })
    return () => {
      window.cancelAnimationFrame(focusFrame)
      document.removeEventListener('keydown', closeOnEscape)
      document.removeEventListener('pointerdown', closeOnOutsidePress)
    }
  }, [open])

  useLayoutEffect(() => {
    const frame = window.requestAnimationFrame(() => moveIndicator(activeIndex))
    const sync = () => moveIndicator(activeIndex)
    window.addEventListener('resize', sync)
    return () => {
      window.cancelAnimationFrame(frame)
      window.removeEventListener('resize', sync)
    }
  }, [activeIndex, locale, moveIndicator])

  return (
    <header ref={headerRef} className="site-header">
      <LiquidGlass macro className="nav-glass" contentClassName="nav-inner">
        <Link className="brand" to={localePath(locale)} aria-label="WinIsland home">
          <img src={appIcon} alt="" />
          <span>WinIsland</span>
        </Link>

        <nav
          id="primary-navigation"
          ref={navRef}
          className={open ? 'primary-nav is-open' : 'primary-nav'}
          aria-label={locale === 'zh' ? '主要导航' : 'Primary navigation'}
          onMouseLeave={() => moveIndicator(activeIndex)}
        >
          <span
            className="nav-indicator"
            aria-hidden="true"
            style={{
              opacity: indicator.visible ? 1 : 0,
              width: indicator.width,
              transform: `translate3d(${indicator.x}px, 0, 0)`,
            }}
          />
          <Link
            ref={(node) => { navLinks.current[0] = node }}
            className={activeIndex === 0 ? 'is-current' : ''}
            aria-current={activeIndex === 0 ? 'page' : undefined}
            to={localePath(locale)}
            onMouseEnter={() => moveIndicator(0)}
          >
            {text.nav.product}
          </Link>
          <Link
            ref={(node) => { navLinks.current[1] = node }}
            className={activeIndex === 1 ? 'is-current' : ''}
            aria-current={activeIndex === 1 ? 'page' : undefined}
            to={localePath(locale, '/winwidget')}
            onMouseEnter={() => moveIndicator(1)}
          >
            {text.nav.winwidget}
          </Link>
          <Link
            ref={(node) => { navLinks.current[2] = node }}
            className={activeIndex === 2 ? 'is-current' : ''}
            aria-current={activeIndex === 2 ? 'page' : undefined}
            to={localePath(locale, '/liquid-glass')}
            onMouseEnter={() => moveIndicator(2)}
          >
            {text.nav.liquid}
          </Link>
          <Link
            ref={(node) => { navLinks.current[3] = node }}
            className={activeIndex === 3 ? 'is-current' : ''}
            aria-current={activeIndex === 3 ? 'page' : undefined}
            to={localePath(locale, '/guide')}
            onMouseEnter={() => moveIndicator(3)}
          >
            {text.nav.guide}
          </Link>
          <Link
            ref={(node) => { navLinks.current[4] = node }}
            className={activeIndex === 4 ? 'is-current' : ''}
            aria-current={activeIndex === 4 ? 'page' : undefined}
            to={localePath(locale, '/changelog')}
            onMouseEnter={() => moveIndicator(4)}
          >
            {text.nav.changelog}
          </Link>
          <a
            ref={(node) => { navLinks.current[5] = node }}
            className="external-nav-link"
            href={GITHUB_URL}
            target="_blank"
            rel="noreferrer"
            aria-label="GitHub (opens in a new tab)"
            onMouseEnter={() => moveIndicator(5)}
          >
            <MarkGithubIcon size={16} />
            <span>GitHub</span>
          </a>
          <Link
            className={currentPath === '/download' ? 'mobile-nav-download is-current' : 'mobile-nav-download'}
            aria-current={currentPath === '/download' ? 'page' : undefined}
            to={localePath(locale, '/download')}
          >
            <Download size={16} />
            <span>{text.nav.download}</span>
          </Link>
        </nav>

        <div className="nav-actions">
          <button
            className="theme-toggle"
            type="button"
            aria-label={themeLabel}
            title={themeLabel}
            onClick={onThemeToggle}
          >
            {theme === 'dark' ? <Sun size={17} /> : <Moon size={17} />}
          </button>
          <Link
            className="language-link"
            to={localePath(otherLocale, currentPath)}
            aria-label={locale === 'en' ? '切换为中文' : 'Switch to English'}
          >
            <Languages size={16} />
            <span>{text.language}</span>
          </Link>
          <Link
            className={currentPath === '/download' ? 'nav-download is-current' : 'nav-download'}
            aria-current={currentPath === '/download' ? 'page' : undefined}
            to={localePath(locale, '/download')}
          >
            {text.nav.download}
          </Link>
          <button
            ref={menuButtonRef}
            className="menu-button"
            type="button"
            aria-label={open
              ? locale === 'zh' ? '关闭导航菜单' : 'Close navigation menu'
              : locale === 'zh' ? '打开导航菜单' : 'Open navigation menu'}
            aria-controls="primary-navigation"
            aria-expanded={open}
            onClick={() => setOpen((value) => !value)}
          >
            {open ? <X size={19} /> : <Menu size={19} />}
          </button>
        </div>
      </LiquidGlass>
    </header>
  )
}

const SHOWCASE_ICONS = [Music2, Bell, LayoutGrid, Palette, Sparkles]
const PLUGIN_ICONS = [MessageSquareText, Disc3, LayoutGrid, SlidersHorizontal, Command, Captions]
const HOME_ACCENT = '#7c8cff'

function HomePage({ locale, theme }: { locale: Locale; theme: SiteTheme }) {
  const text = copy[locale]
  const dark = theme === 'dark'
  const [demoMode, setDemoMode] = useState<DemoMode>('media')
  const capabilities = useMemo(
    () => copy[locale].capabilities.map((label) => ({ node: <span className="site-loop-chip">{label}</span>, title: label })),
    [locale],
  )

  const scrollToMoments = () => {
    document.getElementById('moments')?.scrollIntoView({ behavior: 'smooth' })
  }

  return (
    <>
      <section className="hero section-shell">
        <div className="site-aurora" aria-hidden="true">
          <Aurora colorStops={['#4ca7ff', '#a96aff', '#ff8ac4']} speed={0.6} blend={0.6} layers={2} height={0.42} opacity={dark ? 0.75 : 0.45} dpr={1} lightMode={!dark} />
        </div>
        <div className="hero-copy">
          <ShinyText
            className="lg-eyebrow site-eyebrow"
            text={text.hero.eyebrow}
            color={dark ? '#a1a1aa' : '#6e6e73'}
            shineColor={dark ? '#ffffff' : '#1d1d1f'}
            speed={3}
          />
          <h1 className={locale === 'zh' ? 'hero-title--zh' : undefined}>
            {locale === 'zh'
              ? text.hero.title.split('\n').map((line) => <span key={line}>{line}</span>)
              : text.hero.title}
          </h1>
          <p className="hero-body">{text.hero.body}</p>
          <div className="hero-actions">
            <Magnet padding={40} magnetStrength={3}>
              <StarBorder
                as={Link}
                className="site-star-button"
                to={localePath(locale, '/download')}
                radius={24}
                color="#bfe2ff"
                theme={theme}
                backgroundColor="var(--blue)"
                textColor="#fff"
              >
                <Download size={18} />
                {text.hero.primary}
              </StarBorder>
            </Magnet>
            <Magnet padding={40} magnetStrength={3}>
              <Button
                className="apple-button apple-button--secondary"
                large
                rounded
                tonal
                onClick={scrollToMoments}
              >
                {text.hero.secondary}
                <ChevronRight size={17} />
              </Button>
            </Magnet>
          </div>
        </div>

        <ProductDemo locale={locale} variant="hero" />
      </section>

      <div className="site-loop">
        <LogoLoop logos={capabilities} speed={46} logoHeight={44} gap={12} pauseOnHover fadeOut ariaLabel={text.nav.product} />
      </div>

      <section className="moments-section" id="moments">
        <div className="section-heading section-shell">
          <h2>{text.moments.title}</h2>
          <p>{text.moments.body}</p>
        </div>
        <div className="moments-stage section-shell">
          <Segmented strong rounded className="mode-selector" aria-label="Demo mode">
            {(Object.keys(text.moments.modes) as DemoMode[]).map((mode) => (
              <SegmentedButton
                key={mode}
                active={demoMode === mode}
                onClick={() => setDemoMode(mode)}
              >
                {text.moments.modes[mode]}
              </SegmentedButton>
            ))}
          </Segmented>
          <div className="moments-canvas">
            <ProductDemo locale={locale} mode={demoMode} />
          </div>
        </div>
      </section>

      <section className="site-stack-section">
        <div className="section-heading section-heading--center section-shell">
          <h2>{text.showcase.title}</h2>
          <p>{text.showcase.body}</p>
        </div>
        <div className="section-shell site-stack">
          <ScrollStack useWindowScroll smoothScroll={false} stackPosition="16%" itemDistance={70} itemStackDistance={18} itemScale={0.03} baseScale={0.88} dimAmount={0.1} holdDistance={60}>
            {text.showcase.items.map((item, index) => {
              const Icon = SHOWCASE_ICONS[index]
              return (
                <ScrollStackItem key={item.title} itemClassName="site-stack-card">
                  <div className="site-stack-copy">
                    <small>{item.label}</small>
                    <h3>{item.title}</h3>
                    <p>{item.body}</p>
                    <ul>
                      {item.points.map((point) => <li key={point}><Check size={15} />{point}</li>)}
                    </ul>
                  </div>
                  <div className={`site-stack-visual site-stack-visual--${index}`} aria-hidden="true">
                    {index === 0
                      ? <img src={locale === 'zh' ? compactWanfengZh : compactAfterglowEn} width={locale === 'zh' ? 266 : 397} height={47} alt="" />
                      : <Icon size={84} strokeWidth={1.4} />}
                  </div>
                </ScrollStackItem>
              )
            })}
          </ScrollStack>
        </div>
      </section>

      <section className="features-section section-shell">
        <div className="section-heading section-heading--center">
          <h2>{text.features.title}</h2>
        </div>
        <div className="feature-grid">
          {text.features.items.map((feature, index) => {
            const Icon = [Music2, Waves, Sparkles][index]
            return (
              <SpotlightCard
                key={feature.title}
                className={`feature-card site-feature-card feature-card--${feature.accent}`}
                theme={theme}
                spotlightColor={HOME_ACCENT}
              >
                <div className="feature-card-content">
                  <div className="feature-visual" aria-hidden="true">
                    <GlareHover className="site-feature-glare" width="100%" height="100%" background="transparent" borderColor="transparent" borderRadius="22px" glareColor="#ffffff" glareOpacity={0.35}>
                      {index === 0 && (
                        <img
                          className="feature-island"
                          src={locale === 'zh' ? compactWanfengZh : compactAfterglowEn}
                          width={locale === 'zh' ? 266 : 397}
                          height={47}
                          alt=""
                        />
                      )}
                      {index === 1 && (
                        <div className="motion-orbits">
                          <span /><span /><span />
                          <Orbit />
                        </div>
                      )}
                      {index === 2 && (
                        <div className="widget-icons">
                          <img src={widgetIcon} alt="" />
                          <img src={settingsIcon} alt="" />
                        </div>
                      )}
                    </GlareHover>
                  </div>
                  <span className="feature-icon"><Icon size={20} /></span>
                  <h3>{feature.title}</h3>
                  <p>{feature.body}</p>
                </div>
              </SpotlightCard>
            )
          })}
        </div>
      </section>

      <section className="section-shell site-glass">
        <div className="lg-tilt">
          <TiltedCard
            imageSrc={heroPoster}
            altText={text.glass.imageAlt}
            containerWidth="100%"
            containerHeight="100%"
            imageWidth="100%"
            imageHeight="100%"
            radius={30}
            rotateAmplitude={8}
            scaleOnHover={1.03}
            showTooltip={false}
          />
        </div>
        <div className="site-glass-copy">
          <h2>{text.glass.title}</h2>
          <p>{text.glass.body}</p>
          <ul>
            {text.glass.points.map((point) => <li key={point}><Check size={16} />{point}</li>)}
          </ul>
          <Link className="site-text-link" to={localePath(locale, '/liquid-glass')}>
            {text.glass.action}<ArrowRight size={16} />
          </Link>
        </div>
      </section>

      <section className="tech-section section-shell">
        <div className="tech-heading">
          <h2>{text.performance.title}</h2>
          <p>{text.performance.body}</p>
        </div>
        <LiquidGlass macro className="tech-strip" contentClassName="tech-strip-content">
          {text.performance.items.map((item, index) => {
            const Icon = [ShieldCheck, Gauge, Boxes][index]
            return (
              <div className="tech-item" key={item.value}>
                <span><Icon size={19} /></span>
                <div><small>{item.label}</small><strong>{item.value}</strong></div>
                <p>{item.body}</p>
              </div>
            )
          })}
        </LiquidGlass>
      </section>

      <section className="site-plugins section-shell">
        <div className="section-heading section-heading--center">
          <h2>{text.plugins.title}</h2>
          <p>{text.plugins.body}</p>
        </div>
        <div className="site-card-grid">
          {text.plugins.items.map((item, index) => {
            const Icon = PLUGIN_ICONS[index]
            return (
              <SpotlightCard key={item.title} className="lg-step" theme={theme} spotlightColor={HOME_ACCENT}>
                <span className="feature-icon"><Icon size={19} /></span>
                <h3>{item.title}</h3>
                <p>{item.body}</p>
              </SpotlightCard>
            )
          })}
        </div>
      </section>

      <section className="open-section section-shell">
        <div>
          <h2>{text.open.title}</h2>
          <p>{text.open.body}</p>
        </div>
        <div className="open-actions">
          <Button className="apple-button apple-button--dark" rounded large href={GITHUB_URL}>
            <GitFork size={18} />{text.open.source}
          </Button>
          <Link
            className="apple-button apple-button--secondary apple-link-button"
            to={localePath(locale, '/plugin-dev')}
          >
            <Code2 size={18} />{text.open.plugins}
          </Link>
        </div>
      </section>

      <DownloadSection locale={locale} />
    </>
  )
}

function DownloadSection({ locale }: { locale: Locale }) {
  const text = copy[locale].download
  return (
    <section className="download-section">
      <div className="section-shell download-inner">
        <h2>{text.title}</h2>
        <p>{text.body}</p>
        <div className="download-actions">
          <Button className="apple-button apple-button--white" rounded large href={DOWNLOAD_RELEASE}>
            <Download size={18} />{text.stable}
          </Button>
          <Link to={localePath(locale, '/download')}>{text.nightly}<ArrowRight size={16} /></Link>
        </div>
      </div>
    </section>
  )
}

function DownloadPage({ locale }: { locale: Locale }) {
  const text = copy[locale].download
  return (
    <div className="download-page page-top section-shell">
      <div className="page-hero page-hero--download">
        <h1>{text.title}</h1>
        <p>{text.body}</p>
      </div>
      <div className="channel-grid">
        <LiquidGlass macro className="channel-card" contentClassName="channel-card-content">
          <span className="channel-label"><BadgeCheck size={17} />Stable</span>
          <h2>{text.stable}</h2>
          <p>{locale === 'zh' ? '适合大多数用户的稳定版本。' : 'The stable build recommended for most people.'}</p>
          <div className="channel-downloads">
            <Button className="apple-button apple-button--primary" rounded large href={DOWNLOAD_RELEASE}>
              <Download size={18} />{text.stable}
            </Button>
            <a href="https://winisland.cn/WinIsland/download/stable">{text.mirror}</a>
          </div>
        </LiquidGlass>
        <LiquidGlass macro className="channel-card" contentClassName="channel-card-content">
          <span className="channel-label"><Sparkles size={17} />Nightly</span>
          <h2>{text.nightly}</h2>
          <p>{text.nightlyHint}</p>
          <div className="channel-downloads">
            <Button className="apple-button apple-button--secondary" rounded large tonal href={DOWNLOAD_NIGHTLY}>
              <Download size={18} />{text.nightly}
            </Button>
            <a href="https://winisland.cn/WinIsland/download/nightly">{text.mirror}</a>
          </div>
        </LiquidGlass>
      </div>
      <div className="requirements-panel">
        <div><span>01</span><strong>{locale === 'zh' ? '下载安装程序' : 'Download the installer'}</strong></div>
        <div><span>02</span><strong>{locale === 'zh' ? '允许身份注册提示' : 'Approve identity registration'}</strong></div>
      </div>
      <p className="requirements-note">
        {locale === 'zh'
          ? '系统要求：Windows 10 2004 或更高版本 / Windows 11，64 位架构，支持 Skia 的现代显卡。'
          : 'System requirements: Windows 10 version 2004 or later / Windows 11, 64-bit architecture, and a modern GPU with Skia support.'}
      </p>
    </div>
  )
}

function NotFound({ locale }: { locale: Locale }) {
  const text = copy[locale].notFound
  return (
    <div className="not-found page-top section-shell">
      <span>404</span>
      <h1>{text.title}</h1>
      <p>{text.body}</p>
      <Link className="apple-button apple-button--primary apple-link-button" to={localePath(locale)}>
        {text.action}
      </Link>
    </div>
  )
}

function SiteFooter({ locale }: { locale: Locale }) {
  const text = copy[locale]
  return (
    <footer className="site-footer">
      <div className="section-shell footer-grid">
        <div className="footer-brand">
          <Link className="brand" to={localePath(locale)}>
            <img src={appIcon} alt="" /><span>WinIsland</span>
          </Link>
          <p>{text.footer.license}</p>
        </div>
        <div>
          <strong>{text.footer.product}</strong>
          <Link to={localePath(locale)}>{text.nav.product}</Link>
          <Link to={localePath(locale, '/winwidget')}>WinWidget Desktop</Link>
          <Link to={localePath(locale, '/download')}>{text.nav.download}</Link>
        </div>
        <div>
          <strong>{text.footer.resources}</strong>
          <Link to={localePath(locale, '/guide')}>{text.nav.guide}</Link>
          <Link to={localePath(locale, '/liquid-glass')}>Liquid Glass</Link>
          <Link to={localePath(locale, '/plugin-dev')}>{text.docs.pages['plugin-dev']}</Link>
          <Link to={localePath(locale, '/changelog')}>{text.nav.changelog}</Link>
        </div>
        <div>
          <strong>{text.footer.community}</strong>
          <a href={GITHUB_URL} target="_blank" rel="noreferrer">GitHub</a>
          <span>QQ · 435799156</span>
        </div>
      </div>
      <div className="section-shell footer-bottom">
        <span>© 2026 WinIsland</span>
        <span>Made for Windows, inspired by thoughtful design.</span>
      </div>
    </footer>
  )
}

export default App
