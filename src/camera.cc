#include "camera.h"
#include "screen.h"
#include "global.h"

#ifdef __APPLE__
#include <OpenGL/gl.h>
#include <OpenGL/glu.h>
#include <GLUT/glut.h>
#else
#ifdef _WIN32
  #include <windows.h>
#endif
#include <GL/gl.h>
#include <GL/glu.h>
#include <GL/glut.h>
#endif


Camera::Camera()
	: mPosition(100, -100, 700),
	  mView(100, -100, 0),
	  mUp(0, 1, 0),
	  mFixedDistance(700),
	  mFixedDistVOffset(700),
	  mMode(CAMERA_OVERHEAD),
	  mTargetRotation(0), mTargetSpeed(0)
{
}


void Camera::apply()
{
	gluLookAt(mPosition.x, mPosition.y, mPosition.z,
		  mView.x, mView.y, mView.z,
		  mUp.x, mUp.y, mUp.z);

	Global::normal.set(-mUp);
}


void Camera::set(double px, double py, double pz,
		 double vx, double vy, double vz,
		 double ux, double uy, double uz)
{
	mPosition.set(px, py, pz);
	mView.set(vx, vy, vz);
	mUp.set(ux, uy, uz);
}


inline void doRotate(Coord3<double>& npos, Coord3<double>& pos,
		     double angle, double x, double y, double z)
{
	double ct = cos(angle), st = sin(angle);

	npos.x = (ct + (1-ct) * x*x) * pos.x;
	npos.x += ((1-ct)*x*y - z*st) * pos.y;
	npos.x += ((1-ct)*x*z + y*st) * pos.z;

	npos.y = ((1-ct)*x*y+z*st) * pos.x;
	npos.y += (ct+(1-ct)*y*y) * pos.y;
	npos.y += ((1-ct)*y*z-x*st) * pos.z;

	npos.z = ((1-ct)*x*z-y*st) * pos.x;
	npos.z += ((1-ct)*y*z+x*st) * pos.y;
	npos.z += (ct+(1-ct)*z*z) * pos.z;
}


void Camera::rotate(double angle, double x, double y, double z)
{
	Coord3<double> nview, view;

	view = mView - mPosition;
	doRotate(nview, view, angle, x, y, z);
	mView = mPosition + nview;
}


void Camera::rotateAroundPoint(Coord3<double> &center,
			       double angle,
			       double x, double y, double z)
{
	Coord3<double> npos, pos;
	pos = mPosition - center;
	doRotate(npos, pos, angle, x, y, z);
	mPosition = center + npos;
}


void Camera::move(double speed)
{
	Coord3<double> vec;
	vec = mView - mPosition;
}


void Camera::cycle()
{
	switch (mMode)
	{
	case CAMERA_OVERHEAD:
		mMode = CAMERA_OVERHEAD_HIGH;
		break;
	case CAMERA_OVERHEAD_HIGH:
		mMode = CAMERA_FIXED_LOC;
		break;
	case CAMERA_FIXED_LOC:
		mMode = CAMERA_FIXED_DIST;
		break;
	case CAMERA_FIXED_DIST:
		mMode = CAMERA_FPS;
#ifndef NDEBUG
		break;
#endif
	case CAMERA_FPS:
		mMode = CAMERA_OVERHEAD;
		break;
	}
	return;
}


void Camera::update(double dt)
{
	Coord3<double> newPos, newView, newUp, tmp;

	switch (mMode)
	{
	case CAMERA_OVERHEAD:
		newPos.set(mTarget.x, mTarget.y, 800);
		newView.set(mTarget);
		newUp.set(0, 1, 0);
		break;
	case CAMERA_FIXED_DIST:
		newPos = constDist(150);
		newView.set(mTarget.x+150*sin(mTargetRotation),
			    mTarget.y+150*cos(mTargetRotation),
			    100);
		newUp.set(0, 0, 1);
		break;
	case CAMERA_FIXED_LOC:
		newPos.set(0, 0, 700);
		newView.set(mTarget);
		newUp.set(0, 0, 1);
		break;
	case CAMERA_OVERHEAD_HIGH:
		newPos.set((Screen::maxX() >> 1),
			   -(Screen::maxY() >> 1), 5000);
		newView.set(newPos.x, newPos.y, 0);
		newUp.set(0, 1, 0);
		break;
	case CAMERA_FPS:
		newPos.set(mTarget.x-100*sin(mTargetRotation),
			   mTarget.y-100*cos(mTargetRotation), 40);
		newView.set(mTarget.x+1000*sin(mTargetRotation),
			    mTarget.y+1000*cos(mTargetRotation),
			    0);
		newUp.set(0, 0, 1);
		break;
	}

	smoothCameraMove(newPos, newView, newUp, dt);
	apply();
}


// ----------------------------------------------------------------------------
// Generic interpolation


template<class T> inline T interpolate (double alpha, T& x0, T& x1)
{
	T tmp;
	tmp = x0 + ((x1 - x0) * alpha);
	return tmp;
}


// ----------------------------------------------------------------------------
// Constrain a given value (x) to be between two (ordered) bounds: min
// and max.  Returns x if it is between the bounds, otherwise returns
// the nearer bound.

double inline clip (const double x, const double min, const double max)
{
	if (x < min) return min;
	if (x > max) return max;
	return x;
}

// ----------------------------------------------------------------------------
// blends new values into an accumulator to produce a smoothed time series
//
// Modifies its third argument, a reference to the float accumulator holding
// the "smoothed time series."
//
// The first argument (smoothRate) is typically made proportional to "dt" the
// simulation time step.  If smoothRate is 0 the accumulator will not change,
// if smoothRate is 1 the accumulator will be set to the new value with no
// smoothing.  Useful values are "near zero".
//
// Usage:
//         blendIntoAccumulator (dt * 0.4, currentFPS, smoothedFPS);


template<class T>
void inline blendIntoAccumulator (const float smoothRate,
				  T& newValue,
				  T& smoothedAccumulator)
{
	T tmp;
	tmp = interpolate(clip(smoothRate, 0, 1), smoothedAccumulator, newValue);
	smoothedAccumulator.set(tmp);
}


void Camera::smoothCameraMove(Coord3<double> &pos,
			      Coord3<double> &view,
			      Coord3<double> &up,
			      double dt)
{
	double speed = .3;
	double rate = dt * speed;

	Coord3<double> tmpPos, tmpView, tmpUp;

	tmpPos.set(mPosition);
	tmpView.set(mView);
	tmpUp.set(mUp);

	blendIntoAccumulator(rate, pos, tmpPos);
	blendIntoAccumulator(rate, view, tmpView);
	blendIntoAccumulator(rate, up, tmpUp);

	mPosition.set(tmpPos);
	mView.set(tmpView);
	mUp.set(tmpUp);
}


Coord3<double> Camera::constDist(double off)
{
	const bool constrainUp = (mFixedDistVOffset != 0);

	Coord3<double> adjustedPosition(
				mPosition.x,
				constrainUp ? mView.y : mPosition.y,
				mPosition.z);

	Coord3<double> offset;
	offset = adjustedPosition - mView;

	const double distance = offset.length();

	if (distance == 0)
		return mPosition;

	Coord3<double> unitOffset, newOffset;
	unitOffset = offset / distance;

	double xdist = sqrt(square(mFixedDistance) - square(mFixedDistVOffset));

	newOffset = unitOffset * xdist;
	newOffset += mView + Coord3<double>(-sin(mTargetRotation) * (300+off),
					    -cos(mTargetRotation) * (300+off),
					    200);
	return newOffset;
}
