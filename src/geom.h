#ifndef SSC_GEOM_H
#define SSC_GEOM_H

#include "coord.h"

#include <vector>

struct Vertex {
    Coord3<double> vertex,
        normal,
        color;
    Coord2<double> texture;

    void set(const Vertex& v)
    {
        vertex.set(v.vertex);
        normal.set(v.normal);
        color.set(v.color);
        texture.set(v.texture);
    }
};

struct Triangle {
    Vertex v1, v2, v3;

    inline Vertex& operator[](std::size_t i)
    {
        switch (i) {
        case 0:
            return v1;
        case 1:
            return v2;
        case 2:
            return v3;
        default:
            abort();
        }
    }

    Triangle()
    {
    }

    Triangle(const Triangle& t)
    {
        v1.set(t.v1);
        v2.set(t.v2);
        v3.set(t.v3);
    }

    Triangle(Coord3<double>& a, Coord3<double>& b, Coord3<double>& c)
    {
        v1.vertex.set(a);
        v2.vertex.set(b);
        v3.vertex.set(c);
    }

    void calculateNormal()
    {
        v1.normal = (v2.vertex - v1.vertex) ^ (v3.vertex - v1.vertex);
        v1.normal.normalize();
        v3.normal = v2.normal = v1.normal;
    }

    void flip()
    {
        Vertex tmp;
        tmp = v1;
        v1 = v2;
        v2 = tmp;
        calculateNormal();
    }

    inline bool operator<(Triangle const& p) const
    {
        return (v3.vertex != p.v3.vertex) ? (v3.vertex < p.v3.vertex) : (v2.vertex != p.v2.vertex) ? (v2.vertex < p.v2.vertex) : (v1.vertex < p.v1.vertex);
    }

    inline bool operator==(Triangle const& p) const
    {
        return ((v1.vertex == p.v1.vertex) && (v2.vertex == p.v2.vertex) && (v3.vertex == p.v3.vertex));
    }
};

class Mesh {
public:
    std::vector<Triangle> mTriangles;
    bool mHasTexture, mHasColor;
    unsigned int mDisplayList, mTextureId;

    enum MapMode { PLANAR,
                   CYLINDRICAL };

    Mesh()
        : mHasTexture(true)
        , mHasColor(false)
        , mDisplayList(0)
        , mTextureId(0)
    {
    }

    void genDisplayList();
    void draw();
    void drawDisplayList();
    void drawArrays();

    void smooth();
    void normalize(double size = 1);
    void flip();

    void setTexture(unsigned int ti) { mTextureId = ti; }
    void setMapMode(MapMode m);
};

#endif // SSC_GEOM_H
