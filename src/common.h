// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_COMMON_H
#define SSC_COMMON_H

#include <cmath>
#include <cstdlib>
#include <ctime>

#include <cassert>

//! \name Windows Specifics

//@{

#if defined(_Windows) || defined(__WINDOWS__) || \
    defined(__WIN32__) || defined(WIN32) || \
    defined(__WINNT__) || defined(__NT__)
# ifndef WINDOWS
#  define WINDOWS
# endif
#endif

#ifdef WINDOWS

#define rint(x) floor((x) + 0.5)

#define M_PI 3.14159265358979323846264338327950288419716939937510

namespace std
{
	typedef unsigned int size_t;
}

#endif // WINDOWS

//@}

const double D_PI = 2 * M_PI;


inline double RAD(double deg)
{
	return deg / 180.0 * M_PI;
}


inline double DEG(double rad)
{
	return rad * 180.0 / M_PI;
}

inline double square(double n) { return n*n; }


#endif // SSC_COMMON_H
