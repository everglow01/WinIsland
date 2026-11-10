import { useMemo, type CSSProperties } from 'react'
import { Button } from 'konsta/react'
import { LiquidGlass } from '@sohumsuthar/liquid-glass'
import { BatteryCharging, Check, ChevronRight, CloudSun, GitFork, LayoutGrid, Boxes, Calendar, Clock, Gauge, Layers, Moon, Music2, Pin, ShieldCheck } from 'lucide-react'
import heroImage from '../assets/winwidget/hero.webp'
import { copy, WINWIDGET_GITHUB_URL, type Locale } from '../content'
import Aurora from './reactbits/Aurora'
import DotGrid from './reactbits/DotGrid'
import GlareHover from './reactbits/GlareHover'
import GradientText from './reactbits/GradientText'
import LogoLoop from './reactbits/LogoLoop'
import Magnet from './reactbits/Magnet'
import PixelCard from './reactbits/PixelCard'
import ScrollStack, { ScrollStackItem } from './reactbits/ScrollStack'
import SpotlightCard from './reactbits/SpotlightCard'
import TiltedCard from './reactbits/TiltedCard'

const HERO_WIDTH = 1220
const HERO_HEIGHT = 624
const WIDGET_RADIUS = 46
const TITLE_COLORS = ['#ee5f9b', '#f0a35e', '#ee5f9b']

const WIDGET_CROPS = [
  { x: 18, y: 18, width: 384, height: 184, size: 'wide', Icon: CloudSun },
  { x: 418, y: 18, width: 384, height: 184, size: 'wide', Icon: Music2 },
  { x: 18, y: 218, width: 384, height: 384, size: 'large', Icon: Moon },
  { x: 817, y: 218, width: 185, height: 184, size: 'small', Icon: Clock },
  { x: 1018, y: 218, width: 184, height: 184, size: 'small', Icon: Calendar },
  { x: 418, y: 418, width: 384, height: 184, size: 'wide', Icon: BatteryCharging },
] as const

type WidgetCrop = (typeof WIDGET_CROPS)[number]

const cropStyle = ({ x, y, width, height }: WidgetCrop): CSSProperties => ({
  aspectRatio: `${width} / ${height}`,
  backgroundImage: `url(${heroImage})`,
  backgroundSize: `${(HERO_WIDTH / width) * 100}% ${(HERO_HEIGHT / height) * 100}%`,
  backgroundPosition: `${(x / (HERO_WIDTH - width)) * 100}% ${(y / (HERO_HEIGHT - height)) * 100}%`,
  borderRadius: `${(WIDGET_RADIUS / width) * 100}% / ${(WIDGET_RADIUS / height) * 100}%`,
})

export default function WinWidgetPage({ locale, theme }: { locale: Locale; theme: 'light' | 'dark' }) {
  const text = copy[locale].winwidget
  const dark = theme === 'dark'
  const widgetNames = useMemo(
    () => copy[locale].winwidget.widgets.items.map((item, index) => {
      const Icon = WIDGET_CROPS[index].Icon
      return { node: <span className="site-loop-chip"><Icon size={17} />{item.title}</span>, title: item.title }
    }),
    [locale],
  )

  const scrollToWidgets = () => {
    document.getElementById('widgets')?.scrollIntoView({ behavior: 'smooth' })
  }

  return (
    <div className="winwidget-page">
      <section className="ww-hero">
        <div className="ww-aurora" aria-hidden="true">
          <Aurora colorStops={['#ee5f9b', '#f0a35e', '#ee5f9b']} speed={0.6} blend={0.6} layers={2} height={0.42} opacity={dark ? 0.75 : 0.45} dpr={1} lightMode={!dark} />
        </div>
        <div className="ww-dots" aria-hidden="true">
          <DotGrid
            key={theme}
            dotSize={6}
            gap={22}
            baseColor={dark ? '#2b2730' : '#dcd8e2'}
            activeColor="#ee5f9b"
            proximity={150}
            fade={0.9}
          />
        </div>
        <div className="section-shell ww-hero-inner">
          <h1>
            <GradientText className="ww-title-gradient" colors={[...TITLE_COLORS]} animationSpeed={7}>WinWidget Desktop</GradientText>
          </h1>
          <strong className="ww-tagline">{text.tagline}</strong>
          <p className="hero-body">{text.body}</p>
          <div className="hero-actions">
            <Magnet padding={40} magnetStrength={3}>
              <Button className="apple-button apple-button--dark" rounded large href={WINWIDGET_GITHUB_URL}>
                <GitFork size={18} />{text.source}
              </Button>
            </Magnet>
            <Magnet padding={40} magnetStrength={3}>
              <Button className="apple-button apple-button--secondary" large rounded tonal onClick={scrollToWidgets}>
                {text.explore}
                <ChevronRight size={17} />
              </Button>
            </Magnet>
          </div>
          <span className="ww-soon">{text.soon}</span>
          <div className="ww-showcase">
            <TiltedCard
              imageSrc={heroImage}
              altText={text.imageAlt}
              containerWidth="100%"
              containerHeight="100%"
              imageWidth="100%"
              imageHeight="100%"
              radius={30}
              rotateAmplitude={5}
              scaleOnHover={1.02}
              showTooltip={false}
            />
          </div>
        </div>
      </section>

      <div className="site-loop">
        <LogoLoop logos={widgetNames} speed={46} logoHeight={44} gap={12} pauseOnHover fadeOut ariaLabel={text.widgets.title} />
      </div>

      <section className="moments-section" id="widgets">
        <div className="section-heading section-heading--center section-shell">
          <h2>{text.widgets.title}</h2>
          <p>{text.widgets.body}</p>
        </div>
        <div className="section-shell ww-widget-grid">
          {text.widgets.items.map((item, index) => {
            const crop = WIDGET_CROPS[index]
            return (
              <SpotlightCard key={item.title} className="ww-widget-card" theme={theme} spotlightColor="#ee5f9b">
                <div className="ww-widget-visual" aria-hidden="true">
                  <div className={`ww-widget-crop ww-widget-crop--${crop.size}`} style={cropStyle(crop)} />
                </div>
                <span className="feature-icon"><crop.Icon size={20} /></span>
                <h3>{item.title}</h3>
                <p>{item.body}</p>
                <span className="ww-sizes">{item.sizes}</span>
              </SpotlightCard>
            )
          })}
        </div>
      </section>

      <section className="site-stack-section">
        <div className="section-heading section-heading--center section-shell">
          <h2>{text.flow.title}</h2>
          <p>{text.flow.body}</p>
        </div>
        <div className="section-shell site-stack">
          <ScrollStack useWindowScroll smoothScroll={false} stackPosition="16%" itemDistance={70} itemStackDistance={18} itemScale={0.03} baseScale={0.88} dimAmount={0.1} holdDistance={60}>
            {text.flow.steps.map((step, index) => (
              <ScrollStackItem key={step.title} itemClassName="site-stack-card ww-flow-card">
                <div className="site-stack-copy">
                  <small>{String(index + 1).padStart(2, '0')}</small>
                  <h3>{step.title}</h3>
                  <p>{step.body}</p>
                </div>
                <div className="ww-flow-visual" aria-hidden="true">
                  <div className={`ww-widget-crop ww-widget-crop--${WIDGET_CROPS[index].size}`} style={cropStyle(WIDGET_CROPS[index])} />
                </div>
              </ScrollStackItem>
            ))}
          </ScrollStack>
        </div>
      </section>

      <section className="ww-glass section-shell">
        <div className="section-heading section-heading--center">
          <h2>{text.glass.title}</h2>
          <p>{text.glass.body}</p>
        </div>
        <div className="ww-glass-grid">
          {text.glass.items.map((item, index) => (
            <PixelCard key={item.title} className="ww-glass-card" variant={index === 0 ? 'pink' : 'blue'} theme={theme} radius={32} intro={false} opacity={0.55} noFocus>
              <div>
                <small>{item.label}</small>
                <h3>{item.title}</h3>
                <p>{item.body}</p>
                <ul>
                  {item.points.map((point) => <li key={point}><Check size={15} />{point}</li>)}
                </ul>
              </div>
            </PixelCard>
          ))}
        </div>
      </section>

      <section className="features-section section-shell">
        <div className="section-heading section-heading--center">
          <h2>{text.traits.title}</h2>
        </div>
        <div className="ww-trait-grid">
          {text.traits.items.map((item, index) => {
            const Icon = [Layers, LayoutGrid, Pin][index]
            return (
              <LiquidGlass key={item.title} macro className="channel-card" contentClassName="channel-card-content">
                <span className="channel-label"><Icon size={17} />{item.label}</span>
                <h2>{item.title}</h2>
                <p>{item.body}</p>
              </LiquidGlass>
            )
          })}
        </div>
      </section>

      <section className="tech-section section-shell">
        <div className="tech-heading">
          <h2>{text.performance.title}</h2>
          <p>{text.performance.body}</p>
        </div>
        <LiquidGlass macro className="tech-strip" contentClassName="tech-strip-content">
          {text.performance.items.map((item, index) => {
            const Icon = [ShieldCheck, Boxes, Gauge][index]
            return (
              <div className="tech-item" key={item.value}>
                <span><Icon size={19} /></span>
                <div><small>{item.label}</small><strong>{item.value}</strong></div>
                <p>{item.body}</p>
              </div>
            )
          })}
        </LiquidGlass>
        <div className="ww-details">
          {text.performance.details.map((item) => (
            <GlareHover
              key={item.title}
              className="ww-detail"
              width="100%"
              height="100%"
              background="var(--glass-card)"
              borderColor="var(--hairline)"
              borderRadius="20px"
              glareColor={dark ? '#ffffff' : '#ee5f9b'}
              glareOpacity={0.22}
            >
              <div>
                <strong>{item.title}</strong>
                <span>{item.body}</span>
              </div>
            </GlareHover>
          ))}
        </div>
      </section>

      <section className="download-section">
        <div className="section-shell download-inner">
          <h2>{text.closing.title}</h2>
          <p>{text.closing.body}</p>
          <div className="download-actions">
            <Button className="apple-button apple-button--white" rounded large href={WINWIDGET_GITHUB_URL}>
              <GitFork size={18} />{text.source}
            </Button>
          </div>
        </div>
      </section>
    </div>
  )
}
