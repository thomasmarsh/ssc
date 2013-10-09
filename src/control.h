// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_CONTROL_H
#define SSC_CONTROL_H

#include "handler.h"

class Controller
{
public:
	Controller();
	~Controller();

	inline bool wantExit() { bool tmp = mQuit; mQuit = false; return tmp; }
	inline void setQuit() { mQuit = true; }
	inline bool isPaused() { return mPause; }
	inline void unPause() { mPause = false; }
	inline bool slowMotion() { return mSlowMo; }
	void poll(double dt);
	void setHandler(EventHandler *h) { mHandler = h; }
private:
	EventHandler *mHandler;

	bool mQuit, mPause, mSlowMo;
};

#endif // SSC_CONTROL_H
