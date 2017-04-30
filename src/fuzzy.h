// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_FUZZY_H
#define SSC_FUZZY_H

#include <cassert>
#include <cmath>
#include <cstdio>
#include <cstdlib>

inline double min(double a, double b)
{
    if (a < b) {
        return a;
    }
    return b;
}

// A ? B : C
template <typename A, typename B, typename C>
struct IfAndThenRule {
    typename A::MemberFunc a;
    typename B::MemberFunc b;
    typename C::MemberFunc c;
};

inline double Zcurve(double x, double a, double b)
{
    double tmp;

    if ((a < x) && (x <= (a + b))) {
        tmp = a + b - x;
        return (tmp * tmp) / (2 * b * b);
    }

    if ((x >= (a - b)) && (x <= a)) {
        tmp = x - a - b;
        return 1 - ((tmp * tmp) / (2 * b * b));
    }
    abort();
}

inline double Scurve(double x, double a, double b)
{
    double tmp;

    if (((a - b) <= x) && (x <= a)) {
        tmp = x - (a - b);
        return (tmp * tmp) / (2 * b * b);
    }
    if ((a < x) && (x <= (a + b))) {
        tmp = a + b - x;
        return 1 - ((tmp * tmp) / (2 * b * b));
    }
    abort();
}

inline double picurve(double tri[], double value)
{
    if (value == tri[1]) {
        return 1;
    }
    if (value <= tri[0]) {
        return 0;
    }
    if (value >= tri[2]) {
        return 0;
    }

    double a, b, c;

    if (value < tri[1]) {
        a = (tri[1] + tri[0]) / 2;
        b = (tri[1] - tri[0]) * 2;
        return Scurve(value, a, b);
    }
    a = (tri[2] + tri[1]) / 2;
    b = (tri[2] - tri[1]) * 2;
    return Zcurve(value, a, b);
}

inline double membership(double tri[], double value)
{
    double v = 0;
    if (value == tri[1]) {
        v = 1;
    } else if (value <= tri[0]) {
        v = 0;
    } else if (value >= tri[2]) {
        v = 0;
    } else if (value < tri[1]) {
        v = (1 / (tri[1] - tri[0])) * (value - tri[0]);
    } else {
        v = (1 / (tri[2] - tri[1])) * (tri[2] - value);
    }

    //      double p = picurve(tri, value);
    //      fprintf(stderr, "d=%.2f (%.2f, %.2f)\n", fabs(p-v), v, p);
    return v;
}

template <typename A, typename B, typename C>
struct FuzzyRuleBase {
    static IfAndThenRule<A, B, C> rules[A::FUZZY_LAST * B::FUZZY_LAST];
};

template <typename R, typename A, typename B, typename C>
inline double FuzzyControl(double a, double b)
{
    // keep everything within bounds
    if (a < A::MBF[0][0]) {
        a = A::MBF[0][0];
    } else if (a > A::MBF[A::FUZZY_LAST - 1][2]) {
        a = A::MBF[A::FUZZY_LAST - 1][2];
    }
    if (b < B::MBF[0][0]) {
        b = B::MBF[0][0];
    } else if (b > B::MBF[B::FUZZY_LAST - 1][2]) {
        b = B::MBF[B::FUZZY_LAST - 1][2];
    }

    // initialize the container for membership value results
    double squares[B::FUZZY_LAST];
    for (int i = 0; i < B::FUZZY_LAST; ++i) {
        squares[i] = 0;
    }

    // calculate membership values
    for (int i = 0; i < A::FUZZY_LAST; ++i) {
        for (int j = 0; j < B::FUZZY_LAST; ++j) {
            squares[R::rules[i * j].c] += min(membership(A::MBF[i], a),
                membership(B::MBF[j], b));
        }
    }

    double output(0), div(0);

    for (int i = 0; i < C::FUZZY_LAST; ++i) {
        squares[i] = sqrt(squares[i]);
        output += C::MBF[i][0] * squares[i];
        div += squares[i];
    }
    if (div == 0) {
        return 0;
    }
    return output / div;
}

#endif // SSC_FUZZY_H
