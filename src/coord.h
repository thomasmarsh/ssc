// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_COORD_H
#define SSC_COORD_H

#include "common.h"

template <typename T>
class Coord2 {
public:
    T x, y;

    typedef Coord2<T> Self;

    //
    // constructors
    //

    Coord2()
        : x(0)
        , y(0)
    {
    }

    Coord2(T nx, T ny)
        : x(nx)
        , y(ny)
    {
    }

    // copy constructor
    Coord2(Self& other)
        : x(other.x)
        , y(other.y)
    {
    }

    // destructor
    ~Coord2() {}

    //
    // setters
    //

    inline void set(T nx, T ny) { x = nx, y = ny; }
    inline void set(const Self& o) { x = o.x, y = o.y; }

    //
    // operations
    //

    // assignment
    inline Self operator=(const Self& o)
    {
        x = o.x, y = o.y;
        return *this;
    }

    // divide (scalar)
    inline Self operator/(T n) const
    {
        Self result(x / n, y / n);
        return result;
    }

    // multiply (scalar)
    inline Self operator*(T n) const
    {
        Self result(x * n, y * n);
        return result;
    }

    // rshift
    inline Self operator>>(T n) const
    {
        Self result(x >> n, y >> n);
        return result;
    }

    // addition
    inline Self operator+(const Self& o) const
    {
        Self result(x + o.x, y + o.y);
        return result;
    }

    // subtraction
    inline Self operator-(const Self& o)
    {
        Self result(x - o.x, y - o.y);
        return result;
    }

    // unary minus
    inline Self operator-(void) const
    {
        Self result(-x, -y);
        return result;
    }

    // -=
    inline Self operator-=(const Self& o)
    {
        *this = (*this - o);
        return *this;
    }

    // +=
    inline Self operator+=(const Self& o)
    {
        *this = (*this + o);
        return *this;
    }

    // /=
    inline Self operator/=(T n)
    {
        x /= n, y /= n;
        return *this;
    }

    inline T& operator[](std::size_t index)
    {
        switch (index) {
        case 0:
            return x;
        case 1:
            return y;
        default:
            abort();
        }
    }

    T dot(const Self& o) const { return x * o.x + y * o.y; }

    double length() const { return hypot((double)x, (double)y); }
    double angle() const { return atan2((double)x, (double)-y); }

    static const Self zero;
};

template <typename T>
class Coord3 {
public:
    T x, y, z;

    typedef Coord3<T> Self;

    //
    // constructors
    //

    Coord3()
        : x(0)
        , y(0)
        , z(0)
    {
    }

    Coord3(T nx, T ny, T nz)
        : x(nx)
        , y(ny)
        , z(nz)
    {
    }

    // copy constructor
    Coord3(Self& other)
        : x(other.x)
        , y(other.y)
        , z(other.z)
    {
    }

    // copy constructor
    Coord3(const Self& other)
        : x(other.x)
        , y(other.y)
        , z(other.z)
    {
    }

    // destructor
    ~Coord3() {}

    //
    // setters
    //

    inline void set(T nx, T ny, T nz) { x = nx, y = ny, z = nz; }
    inline void set(const Self& o) { x = o.x, y = o.y, z = o.z; }

    //
    // operations
    //

    // assignment
    inline Self operator=(const Self& o)
    {
        x = o.x, y = o.y, z = o.z;
        return *this;
    }

    // divide (scalar)
    inline Self operator/(T n) const
    {
        Self result(x / n, y / n, z / n);
        return result;
    }

    // multiply (scalar)
    inline Self operator*(T n) const
    {
        Self result(x * n, y * n, z * n);
        return result;
    }

    // addition
    inline Self operator+(const Self& o) const
    {
        Self result(x + o.x, y + o.y, z + o.z);
        return result;
    }

    // subtraction
    inline Self operator-(const Self& o)
    {
        Self result(x - o.x, y - o.y, z - o.z);
        return result;
    }

    inline Self operator^(Self const& o) const
    {
        Self result(y * o.z - z * o.y,
            z * o.x - x * o.z,
            x * o.y - y * o.x);
        return result;
    }

    // unary minus
    inline Self operator-(void) const
    {
        Self result(-x, -y, -z);
        return result;
    }

    // -=
    inline Self operator-=(const Self& o)
    {
        *this = (*this - o);
        return *this;
    }

    // +=
    inline Self operator+=(const Self& o)
    {
        *this = (*this + o);
        return *this;
    }

    // /=
    inline Self operator/=(T n)
    {
        x /= n, y /= n, z /= n;
        return *this;
    }

    // comparison operators
    inline bool operator<(Self const& o) const
    {
        return (z != o.z) ? (z < o.z) : (y != o.y) ? (y < o.y) : (x < o.x);
    }

    inline bool operator==(Self const& o) const
    {
        return ((z == o.z) && (y == o.y) && (x == o.x));
    }
    inline bool operator!=(Self const& o) const
    {
        return ((z != o.z) || (y != o.y) || (x != o.x));
    }

    inline T dot(const Self& o) const { return x * o.x + y * o.y + z * o.z; }

    inline double length() const { return sqrt(dot(*this)); }
    inline double angle() const { return atan2((double)x, (double)-y); }

    inline Self normalize()
    {
        T n = sqrt(dot(*this));
        if (n > 0)
            *this /= n;
        return *this;
    }

    inline int hash() const
    {
        T v = x * 1234567 + y * 2345671 + z * 3456712;
        int* ip = (int*)&v;
        return *ip;
    }

    inline T& operator[](std::size_t index)
    {
        switch (index) {
        case 0:
            return x;
        case 1:
            return y;
        case 2:
            return z;
        default:
            abort();
        }
    }

    static const Self zero;
    static const Self side;
    static const Self up;
    static const Self forward;
};

inline double distance(Coord3<double>& a, Coord3<double>& b)
{
    Coord3<double> tmp;
    tmp = a - b;
    return tmp.length();
}

#endif // SSC_COORD_H
