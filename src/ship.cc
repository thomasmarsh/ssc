// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "ship.h"
#include "draw.h"
#include "global.h"
#include "model.h"
#include "glpng.h"

const unsigned int SHIP_SHIELD_RADIUS = 12;
const unsigned int SHIP_RADIUS = 10;
const unsigned int SHIP_MAX_SPEED = 15;
const double SHIP_MASS = 10.0;

bool accelFlag = false;

inline void calcRotationPoints(double *x, double *y, double rot, int radius)
{
	*x = rint(sin(rot) * radius);
	*y = rint(-cos(rot) * radius);
}

int SPHERE;
GLuint ammoFlash1;

Ship::Ship(double x, double y)
	: ScreenObject(PLAYER_TYPE,
		       SHIP_SHIELD_RADIUS,
		       SHIP_MASS,
		       SHIP_MAX_SPEED,
		       x, y)
{
	r = g = 1;
	b = 0;

	init();

	/* make a display list containing a sphere */
	SPHERE = glGenLists(1);
	glNewList(SPHERE, GL_COMPILE);

	GLUquadricObj* quad;
	quad = gluNewQuadric();

	// -------- yellow+alpha ------------

	// ship body
	glPushMatrix();
	  glScalef(1, 1, .5);
	  gluSphere(quad, radius, 20, 20);
	glPopMatrix();

	// thrust cylinder 1
	glPushMatrix();
	  glTranslated(-9, -9, 0);
	  glRotated(90, 0, 1, 0);
	  gluCylinder(quad, 2, 2, 18, 20, 20);
	glPopMatrix();

	// thrust cylinder 2
	glPushMatrix();
	  glTranslated(-9, 9, 0);
	  glRotated(90, 0, 1, 0);
	  gluCylinder(quad, 2, 2, 18, 20, 20);
	glPopMatrix();

	// blaster
	glPushMatrix();
	  glTranslated(9, 0, 0);
	  glRotated(90, 0, 1, 0);
	  gluCylinder(quad, 2, 2, 4, 20, 20);
	glPopMatrix();

	// -------- black ------------

	glColor3f(0, 0, 0);

	// cockpit
	glPushMatrix();
	  glTranslated(1, 0, 4.0);
	  glScalef(1, 1, .2);
	  gluSphere(quad, 8, 10, 10);
	glPopMatrix();

	glColor3f(.4, .4, .4);

	// exhaust
	glPushMatrix();
	  glTranslated(-9, 0, 0);
	  glRotated(-90, 0, 1, 0);
	  glRotated(90, 0, 0, 1);
	  gluCylinder(quad, 4, 2, 6, 20, 20);
	glPopMatrix();

	gluDeleteQuadric(quad);

	glEndList();

	pngInfo tmpInfo;
	ammoFlash1 = pngBind("../data/heroAmmoFlash00.png", PNG_NOMIPMAPS, PNG_ALPHA, &tmpInfo, GL_CLAMP, GL_LINEAR, GL_LINEAR);
	if (!ammoFlash1)
	{
		fprintf(stderr, "can't load ammoflash\n");
		abort();
	}
}

Ship::~Ship()
{
}

void Ship::init()
{
	setState(ALIVE);
	enable();

	rotation = M_PI;
	rx = ry = 0;
	calcRotationPoints(&rx, &ry, rotation, radius);

	setPosition(100, 100, 0);
	setVelocity(0, 0, 0);

	shield.setStrength(1);
	mLife = 1;
	mCanFire = true;
	mMissileLevel = MISSILE_NORMAL;
	mRateOfFire = 10;
}

void Ship::upgrade()
{
	switch (mMissileLevel)
	{
	case MISSILE_NORMAL:
		mMissileLevel = MISSILE_FAST;
		mRateOfFire = 5;
		break;
	case MISSILE_FAST:
		mMissileLevel = MISSILE_TRIPLE;
		break;
	case MISSILE_TRIPLE:
		mMissileLevel = MISSILE_QUINTUPLE;
		break;
	case MISSILE_QUINTUPLE:
		mMissileLevel = MISSILE_FULL;
		break;
	case MISSILE_FULL:
		mMissileLevel = MISSILE_NORMAL;
		mRateOfFire = 10;
		break;
	}
}
		
void Ship::move(double dt)
{
	mFireCounter += dt;
	if (getState() == ALIVE)
		shield.regenerate(dt);
	else if (getState() == DYING)
	{
		if (!explosion.finished)
			explosion.move(dt);
		else init();
	}
	ScreenObject::move(dt);
}



inline void drawQuad(double szx, double szy)
{
	glBegin(GL_TRIANGLE_STRIP);
	glTexCoord2i(1, 1); glNormal3d(0, 0, 1); glVertex3d( szx,   szy, 0);
	glTexCoord2i(0, 1); glNormal3d(0, 0, 1); glVertex3d(-szx,   szy, 0);
	glTexCoord2i(1, 0); glNormal3d(0, 0, 1); glVertex3d( szx,  -szy, 0);
	glTexCoord2i(0, 0); glNormal3d(0, 0, 1); glVertex3d(-szx,  -szy, 0);
	glEnd();
}

void Ship::draw()
{
	if (getState() == ALIVE)
	{
		if (accelFlag)
		{
			double c1f = 1-.65,
			       c2f = -(1-.65),
			       c1[4] = { 0.85, 0.65, 1.00, 0.7 },
			       c2[4] = { 1.00, 0.20, 0.25, 0.7 },
			       esz = 1.0 + c2f;

			glPushMatrix();

			glBlendFunc(GL_SRC_ALPHA, GL_ONE);
			glColor4d(c1[0]*c1f+c2[0]*c2f,
				  c1[1]*c1f+c2[1]*c2f,
				  c1[2]*c1f+c2[2]*c2f,
				  c1[3]*c1f+c2[3]*c2f);
			glColor4d(1,1,1,1);

			glEnable(GL_TEXTURE_2D);
			glDisable(GL_DEPTH_TEST);

			glBindTexture(GL_TEXTURE_2D, ammoFlash1);

			glTranslated(mPosition.x, -mPosition.y, mPosition.z);
			glRotated(DEG(-rotation), 0, 0, 1);
			glPushMatrix();
				glTranslated(0, -24, 0);
				glScaled(10, 10, 0);
				drawQuad(1.3, 5*esz);
			glPopMatrix();
			glColor4d(1,1,1,.5);
			glTranslated(0, -15, 0);
			glRotatef(rand(), 0, 0, 1);
			glScaled(50, 50, 0);
			drawQuad(.85*esz, .6*esz);

			glDisable(GL_TEXTURE_2D);
			glEnable(GL_DEPTH_TEST);
			glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
			glPopMatrix();
		}
		if (shield.getStrength() > 0)
			draw::setColor(.4, .3, .2);
		else
			draw::setColor(.4, .3, .2, .5);

		glPushMatrix();
		glTranslated(mPosition.x, -mPosition.y, mPosition.z);
		glRotated(DEG(-rotation)+90, 0, 0, 1);
		glCallList(SPHERE);
		glPopMatrix();

	}
	else if (getState() == DYING)
		explosion.draw();
}


inline void launch(Ship &ship,
		   double dir,
		   double dist,
		   double ang)
{
	(void) new Missile(ScreenObject::PLAYER_TYPE,
			   ship.rotation+dir,
			   ship.mPosition.x+(dist*sin(ship.rotation+ang)),
			   ship.mPosition.y-(dist*cos(ship.rotation+ang)),
			   ship.mPosition.z,
			   ship.mVelocity.x, ship.mVelocity.y, ship.mVelocity.z);
}

void Ship::fire()
{
	if (mFireCounter >= mRateOfFire)
		mFireCounter = 0;
	else return;

	launch(*this, 0, radius+5, 0);

	if (mMissileLevel >= MISSILE_TRIPLE)
	{
		launch(*this, 0, radius+4, RAD(45));
		launch(*this, 0, radius+4,-RAD(45));

		if (mMissileLevel >= MISSILE_QUINTUPLE)
		{
			launch(*this, RAD(90), radius+4, RAD(90));
			launch(*this,-RAD(90), radius+4,-RAD(90));

			if (mMissileLevel >= MISSILE_FULL)
			{
				launch(*this, RAD(180), radius+4, RAD(180+45));
				launch(*this,-RAD(180), radius+4,-RAD(180+45));
			}
		}
	}
}

extern bool mDrawP;

void Ship::accelerate(double amt)
{
	if (mDrawP) for (int i = 0; i < (rand() % 30); i++)
	{
		float p = RAD((rand() % 61) - 30);
		double r, g, b;
		r = 1;
		g = (double) (rand() % 80) / 100;
		b = (double) (rand() % 30) / 100;
		double t = (double) ((rand() % 120)) / 5.0;

		addParticle(mPosition.x - radius*sin(rotation)+((rand()%7)-3),
			    mPosition.y + radius*cos(rotation)+((rand()%7)-3),
			    mPosition.z + (rand()%7)-3,
			    -sin(rotation+p)*mMaxSpeed*.5,
			    cos(rotation+p)*mMaxSpeed*.5,
			    sin(p) * mMaxSpeed * .5,
			    r, g, b, t);
	}

	ScreenObject::accelerate(amt);
}


void Ship::rotate(double amt)
{
	ScreenObject::rotate(amt);
	calcRotationPoints(&rx, &ry, rotation, radius);
}

extern bool mGodMode;

void Ship::damage(double amt)
{
	if (mGodMode) return;

	static float pos[3] = {0.0, 0.0, 0.0};
	if (shield.getStrength() == 0)
	{
		if (amt > mLife)
		{
			if (isAlive())
			{
				setState(DYING);
				Global::audio->playSound(Audio::EXPLOSION, pos);
				explosion.age = 0;
				explosion.finished = false;
				explosion.init(*this);
			}
			mLife = 0;
		}
		else mLife -= amt;
	}
	else
	{
		Global::audio->playSound(Audio::LIFE_LOSE, pos);
		shield.damage(amt);
	}
}
