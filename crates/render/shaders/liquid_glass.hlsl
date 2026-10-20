// PecoFence Liquid Glass: a Direct2D custom pixel shader.
//
// The plate is a rounded box whose surface rises from the rim to a flat top along a
// convex squircle. Every output pixel refracts a straight-down view ray through that
// surface (Snell's law, one index per colour channel) and samples the wallpaper where
// the ray lands, mixing a sharp and a frosted copy. Fresnel reflection and a specular
// highlight from a fixed light give the rim its light. All lengths are device pixels.
//
// Rebuild after editing: scripts/compile-shaders.sh (fxc /T ps_4_0). The compiled
// liquid_glass.cso and its source hash are checked in next to this file.

Texture2D Sharp : register(t0);
Texture2D Frost : register(t1);
SamplerState SharpSampler : register(s0);
SamplerState FrostSampler : register(s1);

cbuffer Constants : register(b0)
{
    float2 size;        // plate size
    float radius;       // corner radius
    float bezel;        // width of the curved rim; never wider than the radius
    float thickness;    // glass height at the flat top
    float ior;          // index of refraction for green
    float dispersion;   // index spread between red and blue
    float frost;        // 0 sharp .. 1 fully frosted
    float2 light;       // unit vector towards the light, screen space (y down)
    float specular;     // highlight strength
    float fresnel;      // reflection strength at grazing angles
    float maxOffset;    // refraction offset clamp; the input rects are padded by this
    float saturation;   // colour lift of the refracted wallpaper
    float2 pad;
};

// Signed distance to the rounded box (negative inside) and its outward unit gradient.
// The gradient is continuous within `radius` of the edge, which covers the whole bezel.
float roundedBox(float2 p, out float2 outward)
{
    float2 c = p - size * 0.5;
    float2 q = abs(c) - (size * 0.5 - radius);
    float2 s = float2(c.x < 0.0 ? -1.0 : 1.0, c.y < 0.0 ? -1.0 : 1.0);
    if (q.x > 0.0 && q.y > 0.0)
    {
        outward = normalize(q) * s;
    }
    else
    {
        outward = (q.x > q.y) ? float2(s.x, 0.0) : float2(0.0, s.y);
    }
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}

// Squircle profile: h(t) = (1 - (1 - t)^4)^(1/4), vertical at the rim, flat on top.
// Returns the height and the slope dh/d(depth), with the vertical rim clamped.
float2 profile(float depth)
{
    float t = saturate(depth / bezel);
    float u = 1.0 - t;
    float u4 = u * u * u * u;
    float h = pow(max(1.0 - u4, 1e-4), 0.25);
    float slope = (thickness / bezel) * u * u * u * pow(max(1.0 - u4, 1e-4), -0.75);
    return float2(thickness * h, min(slope, 4.0));
}

// Where a straight-down ray through the surface at this pixel meets the wallpaper.
float2 refractedOffset(float3 normal, float height, float eta)
{
    float3 ray = refract(float3(0.0, 0.0, -1.0), normal, eta);
    if (ray.z > -1e-3)
    {
        return float2(0.0, 0.0); // total internal reflection; the rim is lit instead
    }
    float2 offset = ray.xy * (height / -ray.z);
    float len = length(offset);
    return len > maxOffset ? offset * (maxOffset / len) : offset;
}

float3 sampleAt(float4 uv0, float4 uv1, float2 offset)
{
    float3 sharp = Sharp.Sample(SharpSampler, uv0.xy + offset * uv0.zw).rgb;
    float3 soft = Frost.Sample(FrostSampler, uv1.xy + offset * uv1.zw).rgb;
    return lerp(sharp, soft, frost);
}

float4 main(
    float4 pos : SV_POSITION,
    float4 scene : SCENE_POSITION,
    float4 uv0 : TEXCOORD0,
    float4 uv1 : TEXCOORD1) : SV_Target
{
    float2 outward;
    float distance = roundedBox(scene.xy, outward);
    // Pixels past the rim (the compositor clip antialiases the outline) repeat the rim,
    // so the clip blends refracted glass, never unrefracted wallpaper.
    float depth = max(-distance, 0.0);
    // The flat top refracts nothing: one sharp and one frosted read instead of six, no
    // lighting beyond the head-on Fresnel term. Most of a fence's pixels take this path.
    if (depth >= bezel)
    {
        float3 flat = sampleAt(uv0, uv1, float2(0.0, 0.0));
        float flatLuma = dot(flat, float3(0.2126, 0.7152, 0.0722));
        flat = saturate(lerp(flatLuma.xxx, flat, saturation));
        return float4(lerp(flat, float3(1.0, 1.0, 1.0), saturate(0.04 * fresnel)), 1.0);
    }

    float2 hs = profile(depth);
    float3 normal = normalize(float3(outward * hs.y, 1.0));

    float3 color;
    color.r = sampleAt(uv0, uv1, refractedOffset(normal, hs.x, 1.0 / (ior - dispersion))).r;
    color.g = sampleAt(uv0, uv1, refractedOffset(normal, hs.x, 1.0 / ior)).g;
    color.b = sampleAt(uv0, uv1, refractedOffset(normal, hs.x, 1.0 / (ior + dispersion))).b;

    float luma = dot(color, float3(0.2126, 0.7152, 0.0722));
    color = saturate(lerp(luma.xxx, color, saturation));

    // The rim facing the light catches a Blinn-Phong highlight; light leaving the glass
    // brightens the opposite rim more softly. Both vanish on the flat top. The light is
    // low (about 22 degrees up), so only the steep outer rim faces it: a thin line, not a
    // glossy bubble across the bezel.
    float tilt = 1.0 - normal.z;
    float facing = dot(outward, light);
    float3 toLight = normalize(float3(light * 2.5, 1.0));
    float3 halfway = normalize(toLight + float3(0.0, 0.0, 1.0));
    float highlight = pow(saturate(dot(normal, halfway)), 96.0) * saturate(tilt * 4.0);
    float inner = pow(saturate(-facing), 3.0) * tilt * tilt;
    float reflectance = 0.04 + 0.96 * pow(tilt, 5.0);
    color = lerp(color, float3(1.0, 1.0, 1.0), saturate(reflectance * fresnel));
    color += (highlight + inner * 0.35) * specular;
    return float4(saturate(color), 1.0);
}
