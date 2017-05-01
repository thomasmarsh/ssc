// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_MODEL_H
#define SSC_MODEL_H

#include "camera.h"
#include "flock.h"
#include "object.h"
#include "starfield.h"

#include <vector>

class Model {
private:
    Model();

public:
    static inline Model& getInstance()
    {
        static Model instance;
        return instance;
    }

    void addObject(ScreenObject* obj);
    void draw(double dt, bool doMove = true);
    void update(double dt, bool doMove = true);
    void setPlayerPos();
    void clearLevel(bool reset = false);
    void startGame();

    void cycleCameraView() { mCamera.cycle(); }
    inline ScreenObject* getHead() { return mHead; }

private:
    ScreenObject* mHead;
    StarField mStarField;
    Camera mCamera;
    std::vector<std::shared_ptr<Wall>> mWalls;
};

void addParticle(double x, double y, double z,
                 double fx, double fy, double fz,
                 double r = 1, double g = 1, double b = 1,
                 double time = 20);

#endif // SSC_MODEL_H
