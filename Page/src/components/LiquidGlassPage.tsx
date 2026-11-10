import { useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { Button } from 'konsta/react'
import { ChevronRight, Download, GitFork } from 'lucide-react'
import heroPoster from '../assets/media/hero-loop-poster.jpg'
import demoBackdrop from '../assets/glass-demo.svg'
import { copy, GITHUB_URL, localePath, type Locale } from '../content'
import BorderGlow from './reactbits/BorderGlow'
import DotGrid from './reactbits/DotGrid'
import GlareHover from './reactbits/GlareHover'
import GlassSurface from './reactbits/GlassSurface'
import GradientText from './reactbits/GradientText'
import Magnet from './reactbits/Magnet'
import ShinyText from './reactbits/ShinyText'
import SpotlightCard from './reactbits/SpotlightCard'
import TiltedCard from './reactbits/TiltedCard'
import VariableProximity from './reactbits/VariableProximity'

const GLASS_COLORS = ['#5cc8ff', '#a78bfa', '#ff8ac4', '#5cc8ff']
const ACCENT = '#7c8cff'

const CHAPTER_CODE = [
  {
    file: 'capture/interop.rs · capture/desktop.rs',
    code: `GetProcAddress(dwm, PCSTR(163_usize as *const u8)),
GetProcAddress(dwm, PCSTR(164_usize as *const u8)),

create(host, device.as_raw(), &mut visual, &mut handle).ok()?;

(item_factory.vtable().CreateFromVisual)(
    item_factory.as_raw(),
    visual.as_raw(),
    &mut raw_item,
)`,
  },
  {
    file: 'liquid.sksl',
    code: `float2 m = q / c.x;
float2 mp = pow(m, float2(c.y - 1.0));
g = normalize(mp) * sgn;
return (pow(mp.x * m.x + mp.y * m.y, 1.0 / c.y) - 1.0) * c.x;`,
  },
  {
    file: 'liquid.sksl',
    code: `float band = clamp(inward / max(bezelWidth, 1.0), 0.0, 1.0);
float u = 1.0 - band;
float u2 = u * u;
float slope = min(2.0 * u2 * u / sqrt(max(1.0 - u2 * u2, kEpsilon)), kMaxSlope);
float3 normal = normalize(float3(g * slope, 1.0));`,
  },
  {
    file: 'liquid.sksl',
    code: `float3 ray = refract(float3(0.0, 0.0, -1.0), normal,
                     1.0 / max(glassIor, 1.0001));
if (ray.z < 0.0) {
    shift = ray.xy / -ray.z * (thickness + inward * 0.5) * displacementFactor;
}`,
  },
  {
    file: 'liquid.sksl',
    code: `glass = float3(
    background(p + shift * (1.0 + spread)).r,
    background(p + shift).g,
    background(p + shift * (1.0 - spread)).b);`,
  },
  {
    file: 'liquid.sksl',
    code: `float luma = dot(glass, float3(0.2126, 0.7152, 0.0722));
float tintAmount = min(tint.a + legibility * smoothstep(0.35, 0.95, luma), 1.0);
glass = mix(glass, tint.rgb, tintAmount * coverage);

float lit = pow(max(dot(g, lightDir), 0.0), specularShape.y)
    * max(specular.x - falloff, 0.0);`,
  },
  {
    file: 'liquid.sksl',
    code: `float2 shift = float2(0.0);
if (inward < bezelWidth && displacementFactor != 0.0) {
    // Only the rim computes a surface normal and refraction.
}

if (dot(shift, shift) < 0.0025) {
    glass = background(p);
}`,
  },
] as const

export default function LiquidGlassPage({ locale, theme }: { locale: Locale; theme: 'light' | 'dark' }) {
  const text = copy[locale].liquid
  const dark = theme === 'dark'
  const heroRef = useRef<HTMLElement>(null)
  const backdropRef = useRef<HTMLImageElement>(null)
  const [refraction, setRefraction] = useState(120)
  const [dispersion, setDispersion] = useState(10)
  const [blur, setBlur] = useState(4)

  const scrollToDemo = () => {
    document.getElementById('glass-demo')?.scrollIntoView({ behavior: 'smooth' })
  }

  return (
    <div className="liquid-page">
      <section className="ww-hero lg-hero" ref={heroRef}>
        <div className="ww-dots" aria-hidden="true">
          <DotGrid
            key={theme}
            dotSize={6}
            gap={22}
            baseColor={dark ? '#262832' : '#d9dbe6'}
            activeColor={ACCENT}
            proximity={150}
            fade={0.9}
          />
        </div>
        <div className="section-shell ww-hero-inner">
          <ShinyText
            className="lg-eyebrow"
            text={text.eyebrow}
            color={dark ? '#a1a1aa' : '#6e6e73'}
            shineColor={dark ? '#ffffff' : '#1d1d1f'}
            speed={3}
          />
          <h1 aria-label="Liquid Glass">
            <VariableProximity
              label="Liquid Glass"
              containerRef={heroRef}
              fromFontVariationSettings="'wght' 700"
              toFontVariationSettings="'wght' 900"
              radius={140}
              scale={1}
              lift={0}
              toColor={ACCENT}
            />
          </h1>
          <strong className="ww-tagline">{text.tagline}</strong>
          <p className="hero-body">{text.body}</p>
          <div className="hero-actions">
            <Magnet padding={40} magnetStrength={3}>
              <Button className="apple-button apple-button--primary" large rounded onClick={scrollToDemo}>
                {text.tryIt}
                <ChevronRight size={17} />
              </Button>
            </Magnet>
            <Magnet padding={40} magnetStrength={3}>
              <Button className="apple-button apple-button--secondary" large rounded tonal href={GITHUB_URL}>
                <GitFork size={18} />{text.source}
              </Button>
            </Magnet>
          </div>
        </div>
      </section>

      <section className="moments-section" id="glass-demo">
        <div className="section-heading section-heading--center section-shell">
          <h2>{text.demo.title}</h2>
          <p>{text.demo.body}</p>
        </div>
        <div className="section-shell lg-demo">
          <div className="lg-demo-stage">
            <img ref={backdropRef} src={demoBackdrop} alt="" />
            <GlassSurface
              className="lg-demo-glass"
              width="min(380px, 78%)"
              height={170}
              borderRadius={46}
              blur={blur}
              distortionScale={-refraction}
              redOffset={0}
              greenOffset={dispersion}
              blueOffset={dispersion * 2}
              backdrop={backdropRef}
            />
          </div>
          <div className="lg-demo-controls">
            <label>
              <span>{text.demo.refraction}</span>
              <input type="range" min={0} max={240} value={refraction} onChange={(event) => setRefraction(Number(event.target.value))} />
            </label>
            <label>
              <span>{text.demo.dispersion}</span>
              <input type="range" min={0} max={40} value={dispersion} onChange={(event) => setDispersion(Number(event.target.value))} />
            </label>
            <label>
              <span>{text.demo.blur}</span>
              <input type="range" min={0} max={20} value={blur} onChange={(event) => setBlur(Number(event.target.value))} />
            </label>
          </div>
          <p className="requirements-note">{text.demo.note}</p>
        </div>
      </section>

      <section className="features-section section-shell">
        <div className="section-heading section-heading--center">
          <h2>{text.pipeline.title}</h2>
          <p>{text.pipeline.body}</p>
        </div>
        <div className="lg-pipeline">
          {text.pipeline.steps.map((step, index) => (
            <SpotlightCard key={step.title} className="lg-step" theme={theme} spotlightColor={ACCENT}>
              <small>{String(index + 1).padStart(2, '0')}</small>
              <h3>{step.title}</h3>
              <p>{step.body}</p>
            </SpotlightCard>
          ))}
        </div>
      </section>

      <section className="section-shell lg-chapters">
        {text.chapters.map((chapter, index) => (
          <article className="lg-chapter" key={chapter.title}>
            <div className="lg-chapter-copy">
              <GradientText className="lg-chapter-index" colors={[...GLASS_COLORS]} animationSpeed={6}>
                {String(index + 1).padStart(2, '0')}
              </GradientText>
              <h2>{chapter.title}</h2>
              <p>{chapter.body}</p>
            </div>
            <BorderGlow
              className="lg-code"
              backgroundColor="#17191e"
              borderRadius={22}
              glowColor="232 100 75"
              colors={['#5cc8ff', '#a78bfa', '#ff8ac4']}
            >
              <div className="lg-code-inner">
                <span>{CHAPTER_CODE[index].file}</span>
                <pre><code>{CHAPTER_CODE[index].code}</code></pre>
              </div>
            </BorderGlow>
          </article>
        ))}
      </section>

      <section className="section-shell lg-result">
        <div className="section-heading">
          <h2>{text.result.title}</h2>
          <p>{text.result.body}</p>
        </div>
        <div className="lg-result-grid">
          <div className="lg-tilt">
            <TiltedCard
              imageSrc={heroPoster}
              altText={text.result.imageAlt}
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
          <div className="lg-stats">
            {text.result.stats.map((stat) => (
              <GlareHover
                key={stat.label}
                className="lg-stat"
                width="100%"
                height="100%"
                background="var(--glass-card)"
                borderColor="var(--hairline)"
                borderRadius="20px"
                glareColor={dark ? '#ffffff' : '#7c8cff'}
                glareOpacity={0.22}
              >
                <div>
                  <strong>{stat.value}</strong>
                  <span>{stat.label}</span>
                </div>
              </GlareHover>
            ))}
          </div>
        </div>
      </section>

      <section className="download-section">
        <div className="section-shell download-inner">
          <h2>{text.closing.title}</h2>
          <p>{text.closing.body}</p>
          <div className="download-actions">
            <Link className="apple-button apple-button--white apple-link-button" to={localePath(locale, '/download')}>
              <Download size={18} />{text.closing.action}
            </Link>
          </div>
        </div>
      </section>
    </div>
  )
}
